//! A concurrent work-stealing queue for building schedulers.
//!
//! # Comparison with crossbeam-deque
//!
//! This crate is similar in purpose to [`crossbeam-deque`](https://docs.rs/crossbeam-deque), which
//! also provides concurrent work-stealing queues. However there are a few notable differences:
//!
//! - This crate is more high level - work stealing is done automatically when calling `pop`
//! instead of you having to manually call it.
//! - As such, we do not support as much customization as `crossbeam-deque` - but the algorithm
//! itself can be optimized better.
//! - Queues have a fixed number of local queues that they support, and this number cannot grow.
//! - Each local queue has a fixed capacity, unlike `crossbeam-deque` which supports local queue
//! growth. This makes our local queues faster.
//!
//! # Implementation
//!
//! This crate's queue implementation is based off [Tokio's current scheduler]. The idea is that
//! each thread holds a fixed-capacity local queue, and there is also an unbounded global queue
//! accessible by all threads. In the general case each worker thread will only interact with its
//! local queue, avoiding lots of synchronization - but if one worker thread happens to have a
//! lot less work than another, it will be spread out evenly due to work stealing.
//!
//! Additionally, each local queue stores a [non-stealable LIFO slot] to optimize for message
//! passing patterns, so that if one task creates another, that created task will be polled
//! immediately, instead of only much later when it reaches the front of the local queue.
//!
//! [Tokio's current scheduler]: https://tokio.rs/blog/2019-10-scheduler
//! [non-stealable LIFO slot]: https://tokio.rs/blog/2019-10-scheduler#optimizing-for-message-passing-patterns

use concurrent_queue::ConcurrentQueue;
use once_cell::sync::OnceCell;
use std::cell::UnsafeCell as StdUnsafeCell;
use std::collections::hash_map::{DefaultHasher, RandomState};
use std::fmt::{Debug, Formatter};
use std::hash::{BuildHasher, Hasher};
use std::iter::FusedIterator;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::os::raw::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

static GLOBAL_QUEUE: OnceCell<Queue<usize>> = OnceCell::new();

static mut QUEUES: OnceCell<LocalQueues<'static, usize>> = OnceCell::new();

#[repr(C)]
#[derive(Debug)]
pub struct WorkStealQueue {
    inner: LocalQueue<usize>,
}

impl WorkStealQueue {
    pub fn new() -> Self {
        unsafe {
            QUEUES.get_or_init(|| {
                let cpus = num_cpus::get();
                assert!(cpus > 0);
                GLOBAL_QUEUE
                    .get_or_init(|| Queue::new(cpus, 256))
                    .local_queues()
            });
            let local_queues = QUEUES.get_mut().unwrap();
            WorkStealQueue {
                inner: local_queues.next().expect("should never happen"),
            }
        }
    }

    pub fn push_back<T>(&mut self, element: T) {
        let ptr = Box::leak(Box::new(element));
        self.push_back_raw(ptr as *mut _ as *mut c_void);
    }

    pub fn push_back_raw(&mut self, ptr: *mut c_void) {
        self.inner.push_yield(ptr as usize);
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// 如果是闭包，还是要获取裸指针再手动转换，不然类型有问题
    pub fn pop_front_raw(&mut self) -> Option<*mut c_void> {
        self.inner.pop().map(|p| p as *mut c_void)
    }
}

impl Default for WorkStealQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(C)]
pub struct UnsafeCell<T>(StdUnsafeCell<T>);

impl<T> UnsafeCell<T> {
    pub fn new(data: T) -> Self {
        Self(StdUnsafeCell::new(data))
    }
    pub fn with<F: FnOnce(*const T) -> R, R>(&self, f: F) -> R {
        f(self.0.get())
    }
    pub fn with_mut<F: FnOnce(*mut T) -> R, R>(&self, f: F) -> R {
        f(self.0.get())
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct GlobalQueue<T>(ConcurrentQueue<T>);

impl<T> GlobalQueue<T> {
    pub fn new() -> Self {
        Self(ConcurrentQueue::unbounded())
    }
    pub fn push(&self, value: T) {
        let res = self.0.push(value);
        debug_assert!(res.is_ok());
    }
    pub fn pop(&self) -> Option<T> {
        self.0.pop().ok()
    }
}

impl<T> Default for GlobalQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A work queue.
///
/// This implements [`Clone`] and so multiple handles to the queue can be easily created and
/// shared.
#[repr(C)]
#[derive(Debug)]
pub struct Queue<T>(Arc<Shared<T>>);

impl<T> Queue<T> {
    /// Create a new work queue.
    ///
    /// `local_queues` is the number of [`LocalQueue`]s yielded by [`Self::local_queues`]. Typically
    /// you will have a local queue for each thread on a thread pool.
    ///
    /// `local_queue_size` is the number of items that can be stored in each local queue before it
    /// overflows into the global one. You should fine-tune this to your needs.
    ///
    /// # Panics
    ///
    /// This will panic if the local queue size is not a power of two.
    ///
    pub fn new(local_queues: usize, local_queue_size: u16) -> Self {
        assert_eq!(
            local_queue_size.count_ones(),
            1,
            "Queue size is not a power of two"
        );
        let mask = local_queue_size - 1;

        Self(Arc::new(Shared {
            local_queues: (0..local_queues)
                .map(|_| LocalQueueInner {
                    heads: AtomicU32::new(0),
                    tail: AtomicU16::new(0),
                    mask,
                    items: (0..local_queue_size)
                        .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
                        .collect(),
                })
                .collect(),
            global_queue: GlobalQueue::new(),
            stealing_global: AtomicBool::new(false),
            taken_local_queues: AtomicBool::new(false),
            searchers: AtomicUsize::new(0),
        }))
    }

    /// Push an item to the global queue. When one of the local queues empties, they can pick this
    /// item up.
    pub fn push(&self, item: T) {
        self.0.global_queue.push(item);
    }

    /// Iterate over the local queues of this queue.
    ///
    /// # Panics
    ///
    /// This will panic if called more than one time.
    pub fn local_queues(&self) -> LocalQueues<'_, T> {
        assert!(!self.0.taken_local_queues.swap(true, Ordering::Relaxed));

        LocalQueues {
            shared: self,
            index: 0,
            hasher: RandomState::new().build_hasher(),
        }
    }

    /// Get the number of threads that are currently searching for work inside [`pop`](Self::pop).
    ///
    /// If this number is too high, you may wish to avoid calling [`pop`](Self::pop) to reduce
    /// contention.
    #[must_use]
    pub fn searchers(&self) -> usize {
        self.0.searchers.load(Ordering::Relaxed)
    }
}

impl<T> Clone for Queue<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[repr(C)]
#[derive(Debug)]
struct Shared<T> {
    local_queues: Box<[LocalQueueInner<T>]>,
    global_queue: GlobalQueue<T>,
    /// Whether a thread is currently stealing from the global queue. When `true`, threads
    /// should avoid trying to pop from it to reduce contention.
    stealing_global: AtomicBool,
    /// Whether the local queues have already been yielded to the user and so shouldn't be yielded
    /// again.
    taken_local_queues: AtomicBool,
    /// The number of queues searching for work.
    searchers: AtomicUsize,
}

/// The fixed-capacity SP2C queue owned by each local queue.
#[repr(C)]
struct LocalQueueInner<T> {
    /// The two heads (fronts) of the queue, packed into one atomic by `pack_heads` and
    /// `unpack_heads`.
    ///
    /// The first head, the "stealer" head, always lags behind the second head, the "real" head.
    /// Items are popped starting from the real head, but the space between the two heads still
    /// cannot be overwritten by the tail, as it's being read by a stealer.
    heads: AtomicU32,

    /// The back of the queue. Only incremented by the associated queue.
    tail: AtomicU16,

    /// Bitmask applied to the head and tail to obtain the actual indices, so that the atomics can
    /// be incremented and freely overflow outside of the range of the queue itself.
    mask: u16,

    /// The actual items in the queue.
    items: Box<[UnsafeCell<MaybeUninit<T>>]>,
}

unsafe impl<T: Send> Sync for LocalQueueInner<T> {}

impl<T> Debug for LocalQueueInner<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (protected_head, head) = unpack_heads(self.heads.load(Ordering::Acquire));

        f.debug_struct("LocalQueueInner")
            .field("protected_head", &protected_head)
            .field("head", &head)
            .field("tail", &self.tail)
            .field("mask", &format_args!("{:#b}", self.mask))
            .finish()
    }
}

/// Unpack the `heads` value in a `LocalQueueInner`. Returns a tuple of the stealer head and the
/// real head.
fn unpack_heads(heads: u32) -> (u16, u16) {
    ((heads >> 16) as u16, heads as u16)
}

/// Pack the `heads` value in a `LocalQueueInner` from its stealer head and real head.
fn pack_heads(stealer: u16, real: u16) -> u32 {
    (stealer as u32) << 16 | real as u32
}

/// One of the local queues in a [`Queue`].
///
/// You can create this using [`Queue::local_queues`].
#[repr(C)]
#[derive(Debug)]
pub struct LocalQueue<T> {
    /// Special slot that is always popped from first, to optimize for message passing where one
    /// task is blocked on another.
    lifo_slot: Option<T>,
    local: ValidPtr<LocalQueueInner<T>>,
    shared: Queue<T>,
    /// Random number generator used to find which queue to start work stealing from.
    rng: Rng,
}

impl<T> LocalQueue<T> {
    /// Load the tail of the local queue.
    fn local_tail(&mut self) -> u16 {
        // SAFETY: The tail can be loaded without synchronization because only `self` can write to
        // it, and we have an `&mut self`.
        unsafe { *(&self.local.tail as *const AtomicU16).cast() }
    }

    pub fn len(&self) -> usize {
        self.local.tail.load(Ordering::Acquire) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Push an item to the local queue. If the local queue is full, it will move half of its items
    /// to the global queue.
    pub fn push(&mut self, item: T) {
        if let Some(previous) = self.lifo_slot.replace(item) {
            self.push_yield(previous);
        }
    }

    /// Push an item to the local queue, skipping the LIFO slot. This can be used to give other
    /// tasks a chance to run. Otherwise, there's a risk that one task will completely take over a
    /// thread in a push-pop cycle due to the LIFO slot.
    pub fn push_yield(&mut self, item: T) {
        let tail = self.local_tail();

        // We have to use Acquire to make sure that we don't write into memory that is
        // currently being read by work stealers.
        let mut heads = self.local.heads.load(Ordering::Acquire);

        loop {
            let (steal_head, head) = unpack_heads(heads);

            // If the local queue is not full, we can simply push to that.
            if tail.wrapping_sub(steal_head) < self.local.items.len() as u16 {
                let i = tail & self.local.mask;

                self.local.items[usize::from(i)]
                    .with_mut(|slot| unsafe { slot.write(MaybeUninit::new(item)) });

                // Release is necessary to make sure the above write is ordered before accesssing
                // values.
                self.local
                    .tail
                    .store(tail.wrapping_add(1), Ordering::Release);

                return;
            }

            // If no threads are currently stealing, our overflowing local queue will not be
            // drained, so we should push half of it to the global queue.
            //
            // Otherwise (when threads are stealing) we don't want to wait for them to finish, so
            // we just push this single item to the global queue (but we don't need to push any
            // more since we're about to become less full).
            if steal_head == head {
                let half = self.local.items.len() as u16 / 2;
                let res = self.local.heads.compare_exchange_weak(
                    heads,
                    pack_heads(head.wrapping_add(half), head.wrapping_add(half)),
                    // Release is necessary to ensure that any previous writes to `tail` become
                    // visible after this point. If not made visible, we could get into a situation
                    // where a thread reads an older value for `tail` than `heads`, and if `heads`
                    // has advanced beyond the old `tail` by that point it causes all sorts of
                    // issues.
                    Ordering::AcqRel,
                    // Acquire is necessary because on failure we use the new value to update the
                    // head (see the Acquire ordering above).
                    Ordering::Acquire,
                );

                // Moving the head failed because another thread has just stolen some items. This
                // means the queue is less full, so we can retry pushing to the local queue.
                if let Err(new_heads) = res {
                    heads = new_heads;
                    continue;
                }

                // Push half the items in the current queue to the global queue.
                for i in 0..half {
                    let index = head.wrapping_add(i) & self.local.mask;
                    let item = unsafe {
                        self.local.items[usize::from(index)]
                            .with(|slot| slot.read())
                            .assume_init()
                    };
                    self.shared.0.global_queue.push(item);
                }
            }

            self.shared.0.global_queue.push(item);

            return;
        }
    }

    /// Pop an item from the local queue, or steal from the global and sibling queues if it is
    /// empty.
    pub fn pop(&mut self) -> Option<T> {
        // First try to pop from the LIFO slot.
        if let Some(item) = self.lifo_slot.take() {
            return Some(item);
        }

        let tail = self.local_tail();

        // First try to pop from the local queue.
        let res = atomic_u32_fetch_update(
            &self.local.heads,
            // No memory orderings are necessary here as this is the only thread that mutates
            // the data, and it's not currently mutating the data.
            Ordering::Relaxed,
            Ordering::Relaxed,
            |heads| {
                let (steal_head, head) = unpack_heads(heads);
                if head == tail {
                    None
                } else if steal_head == head {
                    // There are no current stealers; update both heads.
                    Some(pack_heads(head.wrapping_add(1), head.wrapping_add(1)))
                } else {
                    // There is currently a stealer; only update the real head, as it's the
                    // stealer's job to update the stealer head later.
                    Some(pack_heads(steal_head, head.wrapping_add(1)))
                }
            },
        );

        let heads = match res {
            // We have successfully popped something from the local queue.
            Ok(heads) => {
                let (_, head) = unpack_heads(heads);
                let i = head & self.local.mask;
                return Some(unsafe {
                    self.local.items[usize::from(i)]
                        .with(|ptr| ptr.read())
                        .assume_init()
                });
            }
            // The local queue is empty.
            Err(heads) => heads,
        };
        let (steal_head, head) = unpack_heads(heads);
        assert_eq!(head, tail);

        // The number of free slots in the queue we can steal into.
        let space = self.local.items.len() as u16 - head.wrapping_sub(steal_head);

        // Now we will try to steal into this queue from various places.

        // TODO: Potentially throttle stealing?
        self.shared
            .0
            .searchers
            // No ordering is necessary because we use this as a hint, not for safety.
            .fetch_add(1, Ordering::Relaxed);

        struct DecrementSearchers<'a>(&'a AtomicUsize);
        impl Drop for DecrementSearchers<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::Relaxed);
            }
        }
        let _decrement_searchers = DecrementSearchers(&self.shared.0.searchers);

        // If there are no threads currently stealing from the global queue, we will steal from it.
        if self
            .shared
            .0
            .stealing_global
            .compare_exchange(
                false,
                true,
                // No ordering is necessary because we use this as a hint, not for safety.
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            let popped_item = self.shared.0.global_queue.pop();

            if popped_item.is_some() {
                // To avoid having to search for items again after we have completed this one, we
                // fill half of our queue with items from the global queue.

                let steal = (self.local.items.len() as u16 / 2).min(space);
                let mut tail = head;
                let end_tail = head.wrapping_add(steal);

                // Ensure that the following mutations of the local queue of items will not occur
                // before stealers have finished reading.
                u32_acquire_fence(&self.local.heads);

                while tail != end_tail {
                    match self.shared.0.global_queue.pop() {
                        Some(item) => {
                            let i = tail & self.local.mask;
                            self.local.items[usize::from(i)]
                                .with_mut(|slot| unsafe { slot.write(MaybeUninit::new(item)) });
                        }
                        None => break,
                    }
                    tail = tail.wrapping_add(1);
                }
                // Release is necessary to make sure the above write is ordered before accesssing
                // values.
                self.local.tail.store(tail, Ordering::Release);
            }

            self.shared
                .0
                .stealing_global
                .store(false, Ordering::Relaxed);

            if let Some(popped_item) = popped_item {
                return Some(popped_item);
            }
        }

        // Steal work from sibling queues starting from a random location.
        let queues = self.shared.0.local_queues.len();
        let start = self.rng.gen_usize_to(queues);

        'sibling_queues: for i in 0..queues {
            let mut i = start + i;
            if i >= queues {
                i -= queues;
            }

            let queue = &self.shared.0.local_queues[i];
            if std::ptr::eq(queue, &*self.local) {
                continue;
            }

            // Acquire is necessary to make sure that the below load of the tail does not occur
            // before the load of the head.
            let mut queue_heads = queue.heads.load(Ordering::Acquire);

            let (old_queue_head, mut queue_head, steal) = loop {
                let (queue_steal_head, queue_head) = unpack_heads(queue_heads);

                // If another thread is already stealing from this queue, don't steal from it.
                if queue_steal_head != queue_head {
                    continue 'sibling_queues;
                }

                // Acquire is necessary so we don't read into items that are currently being
                // written by the thread itself.
                let queue_tail = queue.tail.load(Ordering::Acquire);

                // The number of items that can be stolen.
                let stealable = queue_tail.wrapping_sub(queue_head);

                // The number of items we actually want to steal - this is half of their queue,
                // rounded up.
                let steal = (stealable - stealable / 2).min(space);

                if steal == 0 {
                    continue 'sibling_queues;
                }

                let new_queue_head = queue_head.wrapping_add(steal);

                let res = queue.heads.compare_exchange_weak(
                    queue_heads,
                    // Only move the real head, as we still need to keep the steal head to read
                    // from the queue.
                    pack_heads(queue_head, new_queue_head),
                    // Release isn't necessary here since the above code doesn't access any memory.
                    Ordering::Acquire,
                    // Acquire is necessary when the compare_exchange fails because the result is
                    // used to update the values in `queue_heads`; see the Acquire above.
                    Ordering::Acquire,
                );

                match res {
                    Ok(_) => break (queue_head, new_queue_head, steal),
                    Err(updated_queue_heads) => queue_heads = updated_queue_heads,
                }
            };

            assert_ne!(steal, 0);

            // Ensure that the following mutations of the local queue of items will not occur
            // before stealers of our own queue have finished reading.
            u32_acquire_fence(&self.local.heads);

            // Read the first item separately, as we will be returning it.
            let first_item = unsafe {
                queue.items[usize::from(old_queue_head & queue.mask)]
                    .with(|slot| slot.read())
                    .assume_init()
            };

            // Copy over the stolen items to our queue.
            for i in 1..steal {
                let src = &queue.items[usize::from(old_queue_head.wrapping_add(i) & queue.mask)];
                let dst =
                    &self.local.items[usize::from(head.wrapping_add(i - 1) & self.local.mask)];

                src.with(|src| dst.with_mut(|dst| unsafe { src.copy_to_nonoverlapping(dst, 1) }))
            }

            // Update the steal head to match the real head.
            loop {
                let res = queue.heads.compare_exchange_weak(
                    pack_heads(old_queue_head, queue_head),
                    pack_heads(queue_head, queue_head),
                    // Release is necessary to make sure the above reads are ordered before any
                    // other thread can write to the values.
                    Ordering::Release,
                    // No ordering is necessary because we're not accessing shared mutable state
                    // after this point.
                    Ordering::Relaxed,
                );

                match res {
                    Ok(_) => break,
                    Err(updated_queue_heads) => {
                        let (updated_queue_steal_head, update_queue_head) =
                            unpack_heads(updated_queue_heads);
                        assert_eq!(updated_queue_steal_head, old_queue_head);
                        queue_head = update_queue_head;
                    }
                }
            }

            if steal > 1 {
                // Release is necessary to make sure the above writes are ordered before accessing
                // values.
                self.local
                    .tail
                    .store(tail.wrapping_add(steal - 1), Ordering::Release);
            }

            return Some(first_item);
        }

        // Lastly, pop from the global queue without guarding against contention, since there is
        // nowhere else we can currently get items from.
        self.shared.0.global_queue.pop()
    }

    /// Get the number of threads that are currently searching for work inside [`pop`](Self::pop).
    ///
    /// If this number is too high, you may wish to avoid calling [`pop`](Self::pop) to reduce
    /// contention.
    #[must_use]
    pub fn searchers(&self) -> usize {
        self.shared.searchers()
    }

    /// Get the global queue that is associated with this local queue.
    #[must_use]
    pub fn global(&self) -> &Queue<T> {
        &self.shared
    }
}

/// An iterator over the [`LocalQueue`]s in a [`Queue`]. Created by [`Queue::local_queues`].
#[repr(C)]
#[derive(Debug)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct LocalQueues<'a, T> {
    shared: &'a Queue<T>,
    index: usize,
    hasher: DefaultHasher,
}

impl<T> Iterator for LocalQueues<'_, T> {
    type Item = LocalQueue<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = self.shared.0.local_queues.get(self.index)?;
        self.index += 1;

        Some(LocalQueue {
            lifo_slot: None,
            // SAFETY: The `LocalQueue` stores an `Arc` so this pointer is guaranteed to be valid
            // until the type is dropped.
            local: unsafe { ValidPtr::new(inner) },
            shared: self.shared.clone(),
            rng: Rng {
                state: {
                    self.hasher.write_usize(self.index);
                    self.hasher.finish()
                },
            },
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<T> ExactSizeIterator for LocalQueues<'_, T> {
    fn len(&self) -> usize {
        self.shared.0.local_queues.len() - self.index
    }
}

impl<T> FusedIterator for LocalQueues<'_, T> {}

/// A `*const T` that is guaranteed to always be valid and non-null.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
struct ValidPtr<T: ?Sized>(NonNull<T>);

impl<T: ?Sized> ValidPtr<T> {
    unsafe fn new(ptr: *const T) -> Self {
        Self(NonNull::new_unchecked(ptr as *mut T))
    }
}

impl<T: ?Sized> Deref for ValidPtr<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}

unsafe impl<T: ?Sized + Sync> Send for ValidPtr<T> {}

unsafe impl<T: ?Sized + Sync> Sync for ValidPtr<T> {}

#[cfg(target_pointer_width = "64")]
type DoubleUsize = u128;
#[cfg(target_pointer_width = "32")]
type DoubleUsize = u64;

/// Wyrand RNG.
#[repr(C)]
#[derive(Debug)]
struct Rng {
    state: u64,
}

impl Rng {
    fn gen_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0xA0761D6478BD642F);
        let t = u128::from(self.state) * u128::from(self.state ^ 0xE7037ED1A0B428DB);
        (t >> 64) as u64 ^ t as u64
    }
    fn gen_usize(&mut self) -> usize {
        self.gen_u64() as usize
    }
    fn gen_usize_to(&mut self, to: usize) -> usize {
        // Adapted from https://www.pcg-random.org/posts/bounded-rands.html
        const USIZE_BITS: usize = std::mem::size_of::<usize>() * 8;

        let mut x = self.gen_usize();
        let mut m = ((x as DoubleUsize * to as DoubleUsize) >> USIZE_BITS) as usize;
        let mut l = x.wrapping_mul(to);
        if l < to {
            let t = to.wrapping_neg() % to;
            while l < t {
                x = self.gen_usize();
                m = ((x as DoubleUsize * to as DoubleUsize) >> USIZE_BITS) as usize;
                l = x.wrapping_mul(to);
            }
        }
        m
    }
}

fn atomic_u32_fetch_update<F>(
    atomic: &AtomicU32,
    set_order: Ordering,
    fetch_order: Ordering,
    mut f: F,
) -> Result<u32, u32>
where
    F: FnMut(u32) -> Option<u32>,
{
    let mut prev = atomic.load(fetch_order);
    while let Some(next) = f(prev) {
        match atomic.compare_exchange_weak(prev, next, set_order, fetch_order) {
            Ok(x) => return Ok(x),
            Err(next_prev) => prev = next_prev,
        }
    }
    Err(prev)
}

fn u32_acquire_fence(atomic: &AtomicU32) {
    if cfg!(tsan) {
        // ThreadSanitizer doesn't support fences.
        atomic.load(Ordering::Acquire);
    } else {
        std::sync::atomic::fence(Ordering::Acquire);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    #[test]
    fn rng() {
        let mut rng = Rng { state: 3493858 };

        let mut remaining: HashSet<_> = (0..15).collect();

        while !remaining.is_empty() {
            let value = rng.gen_usize_to(15);
            assert!(value < 15, "{} is not less than 15!", value);
            remaining.remove(&value);
        }
    }

    #[test]
    fn lifo_slot() {
        let queue = Queue::new(1, 2);
        let mut local = queue.local_queues().next().unwrap();

        assert_eq!(local.pop(), None);
        assert_eq!(local.pop(), None);

        local.push(Box::new(5));
        assert_eq!(local.pop(), Some(Box::new(5)));
        assert_eq!(local.pop(), None);
    }

    #[test]
    fn push_many() {
        let queue = Queue::new(1, 2);
        let mut local = queue.local_queues().next().unwrap();

        for i in 0..4 {
            local.push(Box::new(i));
        }
        assert_eq!(local.pop(), Some(Box::new(3)));
        assert_eq!(local.pop(), Some(Box::new(1)));
        assert_eq!(local.pop(), Some(Box::new(0)));
        assert_eq!(local.pop(), Some(Box::new(2)));
        assert_eq!(local.pop(), None);
    }

    #[test]
    fn wrapping() {
        let queue = Queue::new(1, 2);
        let mut local = queue.local_queues().next().unwrap();

        local.push_yield(Box::new(0));

        for i in 0..10 {
            local.push_yield(Box::new(i + 1));

            assert_eq!(local.pop(), Some(Box::new(i)));
        }

        assert_eq!(local.pop(), Some(Box::new(10)));
        assert_eq!(local.pop(), None);
        assert_eq!(local.pop(), None);
    }

    #[test]
    fn steal_global() {
        for &size in &[2, 4, 8, 16, 32, 64] {
            let queue = Queue::new(4, size);

            for i in 0..16 {
                queue.push(Box::new(i));
            }

            let mut local = queue.local_queues().next().unwrap();

            for i in 0..16 {
                assert_eq!(local.pop(), Some(Box::new(i)));
            }

            assert_eq!(local.pop(), None);
        }
    }

    #[test]
    fn steal_siblings() {
        let queue = Queue::new(2, 64);

        let mut locals: Vec<_> = queue.local_queues().collect();

        locals[0].push_yield(Box::new(4));
        locals[0].push_yield(Box::new(5));

        locals[1].push(Box::new(1));
        locals[1].push(Box::new(0));

        queue.push(Box::new(2));
        queue.push(Box::new(3));

        for i in 0..6 {
            assert_eq!(locals[1].pop(), Some(Box::new(i)));
        }
    }

    #[test]
    fn many_locals() {
        let queue = <Queue<()>>::new(10, 128);
        queue.local_queues().for_each(drop);
    }

    #[test]
    fn searchers() {
        let queue = Queue::new(2, 64);
        let mut locals = queue.local_queues();
        let mut local_a = locals.next().unwrap();
        let mut local_b = locals.next().unwrap();

        assert_eq!(local_a.searchers(), 0);
        assert_eq!(local_b.searchers(), 0);

        local_a.push(());
        local_a.push(());
        local_a.pop().unwrap();
        local_a.pop().unwrap();
        queue.push(());
        local_b.pop().unwrap();
        assert!(local_b.pop().is_none());

        assert_eq!(local_a.searchers(), 0);
        assert_eq!(local_b.searchers(), 0);

        // This test hangs on Miri.
        if cfg!(not(miri)) {
            let stop = Arc::new(AtomicBool::new(false));

            let handle = std::thread::spawn({
                let stop = Arc::clone(&stop);
                move || {
                    while !stop.load(Ordering::Relaxed) {
                        local_b.pop();
                    }
                }
            });

            loop {
                let searchers = local_a.searchers();
                assert!(searchers < 2);
                if searchers == 1 {
                    break;
                }
            }

            stop.store(true, Ordering::Relaxed);
            handle.join().unwrap();
        }
    }

    #[test]
    fn stress() {
        let queue = Queue::new(4, 4);

        if cfg!(miri) {
            for _ in 0..3 {
                queue.push(4);
            }
        } else {
            for _ in 0..32 {
                queue.push(6);
            }
        }

        let threads: Vec<_> = queue
            .local_queues()
            .map(|mut queue| {
                std::thread::spawn(move || {
                    while let Some(num) = queue.pop() {
                        for _ in 0..num {
                            queue.push(num - 1);
                        }
                    }
                })
            })
            .collect();

        for thread in threads {
            thread.join().unwrap();
        }
    }

    #[test]
    fn test_work_steal_queue() {
        let mut queue = WorkStealQueue::new();
        assert!(queue.is_empty());
        queue.push_back_raw(1usize as *mut c_void);
        assert_eq!(1, queue.len());
        assert_eq!(1usize as *mut c_void, queue.pop_front_raw().unwrap());
    }
}
