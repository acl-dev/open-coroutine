use std::cell::Cell;

use parking_lot::Mutex;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::Relaxed;

static COUNTER: AtomicU32 = AtomicU32::new(1);

pub(crate) fn seed() -> u64 {
    let rand_state = RandomState::new();

    let mut hasher = rand_state.build_hasher();

    // Hash some unique-ish data to generate some new state
    COUNTER.fetch_add(1, Relaxed).hash(&mut hasher);

    // Get the seed
    hasher.finish()
}

/// A deterministic generator for seeds (and other generators).
///
/// Given the same initial seed, the generator will output the same sequence of seeds.
///
/// Since the seed generator will be kept in a runtime handle, we need to wrap `FastRand`
/// in a Mutex to make it thread safe. Different to the `FastRand` that we keep in a
/// thread local store, the expectation is that seed generation will not need to happen
/// very frequently, so the cost of the mutex should be minimal.
#[repr(C)]
#[derive(Debug)]
pub struct RngSeedGenerator {
    /// Internal state for the seed generator. We keep it in a Mutex so that we can safely
    /// use it across multiple threads.
    state: Mutex<FastRand>,
}

impl RngSeedGenerator {
    /// Returns a new generator from the provided seed.
    #[must_use]
    pub fn new(seed: RngSeed) -> Self {
        Self {
            state: Mutex::new(FastRand::new(seed)),
        }
    }

    /// Returns the next seed in the sequence.
    pub fn next_seed(&self) -> RngSeed {
        let rng = self.state.lock();

        let s = rng.fastrand();
        let r = rng.fastrand();

        RngSeed::from_pair(s, r)
    }

    /// Directly creates a generator using the next seed.
    #[must_use]
    pub fn next_generator(&self) -> Self {
        RngSeedGenerator::new(self.next_seed())
    }
}

impl Default for RngSeedGenerator {
    fn default() -> Self {
        Self::new(RngSeed::new())
    }
}

/// A seed for random number generation.
///
/// In order to make certain functions within a runtime deterministic, a seed
/// can be specified at the time of creation.
#[allow(unreachable_pub)]
#[derive(Debug, Copy, Clone)]
pub struct RngSeed {
    s: u32,
    r: u32,
}

impl RngSeed {
    /// Creates a random seed using loom internally.
    #[must_use]
    pub fn new() -> Self {
        Self::from_u64(seed())
    }

    #[allow(clippy::cast_possible_truncation)]
    fn from_u64(seed: u64) -> Self {
        let one = (seed >> 32) as u32;
        let mut two = seed as u32;

        if two == 0 {
            // This value cannot be zero
            two = 1;
        }

        Self::from_pair(one, two)
    }

    fn from_pair(s: u32, r: u32) -> Self {
        Self { s, r }
    }
}

impl Default for RngSeed {
    fn default() -> Self {
        Self::new()
    }
}

/// Fast random number generate.
///
/// Implement xorshift64+: 2 32-bit xorshift sequences added together.
/// Shift triplet `[17,7,16]` was calculated as indicated in Marsaglia's
/// Xorshift paper: <https://www.jstatsoft.org/article/view/v008i14/xorshift.pdf>
/// This generator passes the `SmallCrush` suite, part of `TestU01` framework:
/// <http://simul.iro.umontreal.ca/testu01/tu01.html>
#[repr(C)]
#[derive(Debug)]
pub struct FastRand {
    one: Cell<u32>,
    two: Cell<u32>,
}

impl FastRand {
    /// Initializes a new, thread-local, fast random number generator.
    #[must_use]
    pub fn new(seed: RngSeed) -> FastRand {
        FastRand {
            one: Cell::new(seed.s),
            two: Cell::new(seed.r),
        }
    }

    /// Replaces the state of the random number generator with the provided seed, returning
    /// the seed that represents the previous state of the random number generator.
    ///
    /// The random number generator will become equivalent to one created with
    /// the same seed.
    pub fn replace_seed(&self, seed: RngSeed) -> RngSeed {
        let old_seed = RngSeed::from_pair(self.one.get(), self.two.get());

        _ = self.one.replace(seed.s);
        _ = self.two.replace(seed.r);

        old_seed
    }

    pub fn fastrand_n(&self, n: u32) -> u32 {
        // This is similar to fastrand() % n, but faster.
        // See https://lemire.me/blog/2016/06/27/a-fast-alternative-to-the-modulo-reduction/
        let mul = (u64::from(self.fastrand())).wrapping_mul(u64::from(n));
        (mul >> 32) as u32
    }

    fn fastrand(&self) -> u32 {
        let mut s1 = self.one.get();
        let s0 = self.two.get();

        s1 ^= s1 << 17;
        s1 = s1 ^ s0 ^ s1 >> 7 ^ s0 >> 16;

        self.one.set(s0);
        self.two.set(s1);

        s0.wrapping_add(s1)
    }
}
