use crate::common::{page_size, Current, Named};
use crate::constants::CoroutineState;
use crate::coroutine::local::{CoroutineLocal, HasCoroutineLocal};
use crate::coroutine::suspender::{DelaySuspender, Suspender, SuspenderImpl};
use corosensei::stack::DefaultStack;
use corosensei::{CoroutineResult, ScopedCoroutine};
use std::cell::Cell;
use std::fmt::Debug;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::panic::UnwindSafe;

#[repr(C)]
pub struct CoroutineImpl<'c, Param, Yield, Return>
where
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    name: String,
    sp: ScopedCoroutine<'c, Param, Yield, Return, DefaultStack>,
    state: Cell<CoroutineState<Yield, Return>>,
    context: CoroutineLocal,
    yields: MaybeUninit<ManuallyDrop<Yield>>,
    //调用用户函数的返回值
    result: MaybeUninit<ManuallyDrop<Return>>,
}

impl<'c, Param, Yield, Return> Drop for CoroutineImpl<'c, Param, Yield, Return>
where
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    fn drop(&mut self) {
        //for test_yield case
        if self.sp.started() && !self.sp.done() {
            unsafe { self.sp.force_reset() };
        }
    }
}

unsafe impl<'c, Param, Yield, Return> Send for CoroutineImpl<'c, Param, Yield, Return>
where
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
}

impl<'c, Param, Yield, Return> CoroutineImpl<'c, Param, Yield, Return>
where
    Param: UnwindSafe,
    Yield: UnwindSafe + Copy + Eq + PartialEq + Debug,
    Return: UnwindSafe + Copy + Eq + PartialEq + Debug,
{
    pub fn new<F>(name: String, f: F, size: usize) -> std::io::Result<Self>
    where
        F: FnOnce(&dyn Suspender<Resume = Param, Yield = Yield>, Param) -> Return,
        F: 'c,
    {
        let stack = DefaultStack::new(size.max(page_size()))?;
        let sp = ScopedCoroutine::with_stack(stack, |y, p| {
            let suspender = SuspenderImpl(y);
            SuspenderImpl::<Param, Yield>::init_current(&suspender);
            let r = f(&suspender, p);
            SuspenderImpl::<Param, Yield>::clean_current();
            r
        });
        Ok(CoroutineImpl {
            name,
            sp,
            state: Cell::new(CoroutineState::Created),
            context: CoroutineLocal::default(),
            yields: MaybeUninit::uninit(),
            result: MaybeUninit::uninit(),
        })
    }

    pub fn state(&self) -> CoroutineState<Yield, Return> {
        self.state.get()
    }

    pub fn change_state(
        &self,
        state: CoroutineState<Yield, Return>,
    ) -> CoroutineState<Yield, Return> {
        let old = self.state.replace(state);
        crate::info!("co {} change state {}->{}", self.get_name(), old, state);
        old
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            self.state(),
            CoroutineState::Complete(_) | CoroutineState::Error(_)
        )
    }

    pub fn get_result(&self) -> Option<Return> {
        if self.is_finished() {
            unsafe {
                let mut m = self.result.assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            }
        } else {
            None
        }
    }

    pub fn get_yield(&self) -> Option<Yield> {
        match self.state() {
            CoroutineState::SystemCall(_, _, _) | CoroutineState::Suspend(_, _) => unsafe {
                let mut m = self.yields.assume_init_read();
                Some(ManuallyDrop::take(&mut m))
            },
            _ => None,
        }
    }

    pub fn resume_with(&mut self, arg: Param) -> CoroutineState<Yield, Return> {
        let mut current = self.state();
        match current {
            CoroutineState::Complete(x) => {
                return CoroutineState::Complete(x);
            }
            CoroutineState::SystemCall(_, _, _) => {}
            CoroutineState::Created | CoroutineState::Ready | CoroutineState::Suspend(_, 0) => {
                current = CoroutineState::Running;
                _ = self.change_state(current);
            }
            _ => panic!("{} unexpected state {current}", self.get_name()),
        };
        CoroutineImpl::<Param, Yield, Return>::init_current(self);
        let state = match self.sp.resume(arg) {
            CoroutineResult::Return(r) => {
                self.result = MaybeUninit::new(ManuallyDrop::new(r));
                let state = CoroutineState::Complete(r);
                assert_eq!(CoroutineState::Running, self.change_state(state));
                state
            }
            CoroutineResult::Yield(y) => {
                self.yields = MaybeUninit::new(ManuallyDrop::new(y));
                let mut current = self.state();
                match current {
                    CoroutineState::Running => {
                        current =
                            CoroutineState::Suspend(y, SuspenderImpl::<Yield, Param>::timestamp());
                        assert_eq!(CoroutineState::Running, self.change_state(current));
                        current
                    }
                    CoroutineState::SystemCall(v, syscall_name, state) => {
                        CoroutineState::SystemCall(v, syscall_name, state)
                    }
                    _ => panic!("{} unexpected state {current}", self.get_name()),
                }
            }
        };
        CoroutineImpl::<Param, Yield, Return>::clean_current();
        state
    }
}

impl<'c, Param, Yield, Return> HasCoroutineLocal for CoroutineImpl<'c, Param, Yield, Return>
where
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    fn local(&self) -> &CoroutineLocal {
        &self.context
    }
}

impl<'c, Yield, Return> CoroutineImpl<'c, (), Yield, Return>
where
    Yield: Copy + Eq + PartialEq + UnwindSafe + Debug,
    Return: Copy + Eq + PartialEq + UnwindSafe + Debug,
{
    pub fn resume(&mut self) -> CoroutineState<Yield, Return> {
        self.resume_with(())
    }
}

impl<'c, Param, Yield, Return> Named for CoroutineImpl<'c, Param, Yield, Return>
where
    Yield: Copy + Eq + PartialEq + UnwindSafe,
    Return: Copy + Eq + PartialEq + UnwindSafe,
{
    fn get_name(&self) -> &str {
        &self.name
    }
}
