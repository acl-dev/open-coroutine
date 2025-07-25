use crate::common::constants::CoroutineState;
use crate::coroutine::listener::Listener;
use crate::coroutine::local::CoroutineLocal;
use crate::coroutine::suspender::Suspender;
use crate::coroutine::StackInfo;
use crate::{catch, warn};
use corosensei::stack::{DefaultStack, Stack};
use corosensei::trap::TrapHandlerRegs;
use corosensei::CoroutineResult;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::VecDeque;
use std::ffi::c_longlong;
use std::fmt::Debug;
use std::io::Error;

cfg_if::cfg_if! {
    if #[cfg(unix)] {
        use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    } else if #[cfg(windows)] {
        use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, EXCEPTION_STACK_OVERFLOW};
        use windows_sys::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS};
    }
}

/// Use `corosensei` as the low-level coroutine.
#[repr(C)]
pub struct Coroutine<'c, Param, Yield, Return> {
    pub(crate) name: String,
    inner: corosensei::Coroutine<Param, Yield, Result<Return, &'static str>, DefaultStack>,
    pub(crate) state: Cell<CoroutineState<Yield, Return>>,
    stack_infos: UnsafeCell<VecDeque<StackInfo>>,
    pub(crate) listeners: VecDeque<&'c dyn Listener<Yield, Return>>,
    pub(crate) local: CoroutineLocal<'c>,
    pub(crate) priority: Option<c_longlong>,
}

impl<'c, Param, Yield, Return> Coroutine<'c, Param, Yield, Return> {
    cfg_if::cfg_if! {
        if #[cfg(unix)] {
            #[allow(clippy::too_many_lines)]
            extern "C" fn trap_handler(
                _signum: libc::c_int,
                _siginfo: *mut libc::siginfo_t,
                context: *mut std::ffi::c_void,
            ) {
                unsafe {
                    //do not disable clippy::transmute_ptr_to_ref
                    #[allow(clippy::transmute_ptr_to_ref)]
                    let context: &mut libc::ucontext_t = std::mem::transmute(context);
                    cfg_if::cfg_if! {
                        if #[cfg(all(
                            any(target_os = "linux", target_os = "android"),
                            target_arch = "x86_64",
                        ))] {
                            let sp = u64::try_from(context.uc_mcontext.gregs[usize::try_from(libc::REG_RSP).expect("overflow")]).expect("overflow");
                        } else if #[cfg(all(
                            any(target_os = "linux", target_os = "android"),
                            target_arch = "x86",
                        ))] {
                            let sp = u64::from(std::ffi::c_uint::from_ne_bytes(context.uc_mcontext.gregs[usize::try_from(libc::REG_ESP).expect("overflow")].to_ne_bytes()));
                        } else if #[cfg(all(target_vendor = "apple", target_arch = "x86_64"))] {
                            let sp = u64::try_from((*context.uc_mcontext).__ss.__rsp).expect("overflow");
                        } else if #[cfg(all(
                                any(target_os = "linux", target_os = "android"),
                                target_arch = "aarch64",
                            ))] {
                            let sp = u64::try_from(context.uc_mcontext.sp).expect("overflow");
                        } else if #[cfg(all(
                            any(target_os = "linux", target_os = "android"),
                            target_arch = "arm",
                        ))] {
                            let sp = u64::try_from(context.uc_mcontext.arm_sp).expect("overflow");
                        } else if #[cfg(all(
                            any(target_os = "linux", target_os = "android"),
                            any(target_arch = "riscv64", target_arch = "riscv32"),
                        ))] {
                            let sp = u64::try_from(context.uc_mcontext.__gregs[libc::REG_SP]).expect("overflow");
                        } else if #[cfg(all(target_vendor = "apple", target_arch = "aarch64"))] {
                            let sp = (*context.uc_mcontext).__ss.__sp;
                        } else if #[cfg(all(target_os = "linux", target_arch = "loongarch64"))] {
                            let sp = u64::try_from(context.uc_mcontext.__gregs[3]).expect("overflow");
                        } else {
                            compile_error!("Unsupported platform");
                        }
                    }
                    if let Some(co) = Self::current() {
                        let stack_ptr_in_bounds = co.stack_ptr_in_bounds(sp);
                        let regs = co.inner.trap_handler().setup_trap_handler(move || {
                            Err(if stack_ptr_in_bounds {
                                "invalid memory reference"
                            } else {
                                "stack overflow"
                            })
                        });
                        cfg_if::cfg_if! {
                            if #[cfg(all(
                                    any(target_os = "linux", target_os = "android"),
                                    target_arch = "x86_64",
                                ))] {
                                let TrapHandlerRegs { rip, rsp, rbp, rdi, rsi } = regs;
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_RIP).expect("overflow")] = c_longlong::try_from(rip).expect("overflow");
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_RSP).expect("overflow")] = c_longlong::try_from(rsp).expect("overflow");
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_RBP).expect("overflow")] = c_longlong::try_from(rbp).expect("overflow");
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_RDI).expect("overflow")] = c_longlong::try_from(rdi).expect("overflow");
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_RSI).expect("overflow")] = c_longlong::try_from(rsi).expect("overflow");
                            } else if #[cfg(all(
                                any(target_os = "linux", target_os = "android"),
                                target_arch = "x86",
                            ))] {
                                let TrapHandlerRegs { eip, esp, ebp, ecx, edx } = regs;
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_EIP).expect("overflow")] = i32::from_ne_bytes(eip.to_ne_bytes());
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_ESP).expect("overflow")] = i32::from_ne_bytes(esp.to_ne_bytes());
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_EBP).expect("overflow")] = i32::from_ne_bytes(ebp.to_ne_bytes());
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_ECX).expect("overflow")] = i32::from_ne_bytes(ecx.to_ne_bytes());
                                context.uc_mcontext.gregs[usize::try_from(libc::REG_EDX).expect("overflow")] = i32::from_ne_bytes(edx.to_ne_bytes());
                            } else if #[cfg(all(target_vendor = "apple", target_arch = "x86_64"))] {
                                let TrapHandlerRegs { rip, rsp, rbp, rdi, rsi } = regs;
                                (*context.uc_mcontext).__ss.__rip = rip;
                                (*context.uc_mcontext).__ss.__rsp = rsp;
                                (*context.uc_mcontext).__ss.__rbp = rbp;
                                (*context.uc_mcontext).__ss.__rdi = rdi;
                                (*context.uc_mcontext).__ss.__rsi = rsi;
                            } else if #[cfg(all(
                                    any(target_os = "linux", target_os = "android"),
                                    target_arch = "aarch64",
                                ))] {
                                let TrapHandlerRegs { pc, sp, x0, x1, x29, lr } = regs;
                                context.uc_mcontext.pc = pc;
                                context.uc_mcontext.sp = sp;
                                context.uc_mcontext.regs[0] = x0;
                                context.uc_mcontext.regs[1] = x1;
                                context.uc_mcontext.regs[29] = x29;
                                context.uc_mcontext.regs[30] = lr;
                            } else if #[cfg(all(
                                    any(target_os = "linux", target_os = "android"),
                                    target_arch = "arm",
                                ))] {
                                let TrapHandlerRegs {
                                    pc,
                                    r0,
                                    r1,
                                    r7,
                                    r11,
                                    r13,
                                    r14,
                                    cpsr_thumb,
                                    cpsr_endian,
                                } = regs;
                                context.uc_mcontext.arm_pc = pc;
                                context.uc_mcontext.arm_r0 = r0;
                                context.uc_mcontext.arm_r1 = r1;
                                context.uc_mcontext.arm_r7 = r7;
                                context.uc_mcontext.arm_fp = r11;
                                context.uc_mcontext.arm_sp = r13;
                                context.uc_mcontext.arm_lr = r14;
                                if cpsr_thumb {
                                    context.uc_mcontext.arm_cpsr |= 0x20;
                                } else {
                                    context.uc_mcontext.arm_cpsr &= !0x20;
                                }
                                if cpsr_endian {
                                    context.uc_mcontext.arm_cpsr |= 0x200;
                                } else {
                                    context.uc_mcontext.arm_cpsr &= !0x200;
                                }
                            } else if #[cfg(all(
                                any(target_os = "linux", target_os = "android"),
                                any(target_arch = "riscv64", target_arch = "riscv32"),
                            ))] {
                                let TrapHandlerRegs { pc, ra, sp, a0, a1, s0 } = regs;
                                context.uc_mcontext.__gregs[libc::REG_PC] = std::ffi::c_ulong::try_from(pc).expect("overflow");
                                context.uc_mcontext.__gregs[libc::REG_RA] = std::ffi::c_ulong::try_from(ra).expect("overflow");
                                context.uc_mcontext.__gregs[libc::REG_SP] = std::ffi::c_ulong::try_from(sp).expect("overflow");
                                context.uc_mcontext.__gregs[libc::REG_A0] = std::ffi::c_ulong::try_from(a0).expect("overflow");
                                context.uc_mcontext.__gregs[libc::REG_A0 + 1] = std::ffi::c_ulong::try_from(a1).expect("overflow");
                                context.uc_mcontext.__gregs[libc::REG_S0] = std::ffi::c_ulong::try_from(s0).expect("overflow");
                            } else if #[cfg(all(target_vendor = "apple", target_arch = "aarch64"))] {
                                let TrapHandlerRegs { pc, sp, x0, x1, x29, lr } = regs;
                                (*context.uc_mcontext).__ss.__pc = pc;
                                (*context.uc_mcontext).__ss.__sp = sp;
                                (*context.uc_mcontext).__ss.__x[0] = x0;
                                (*context.uc_mcontext).__ss.__x[1] = x1;
                                (*context.uc_mcontext).__ss.__fp = x29;
                                (*context.uc_mcontext).__ss.__lr = lr;
                            } else if #[cfg(all(target_os = "linux", target_arch = "loongarch64"))] {
                                let TrapHandlerRegs { pc, sp, a0, a1, fp, ra } = regs;
                                context.uc_mcontext.__pc = pc;
                                context.uc_mcontext.__gregs[1] = ra;
                                context.uc_mcontext.__gregs[3] = sp;
                                context.uc_mcontext.__gregs[4] = a0;
                                context.uc_mcontext.__gregs[5] = a1;
                                context.uc_mcontext.__gregs[22] = fp;
                            } else {
                                compile_error!("Unsupported platform");
                            }
                        }
                    }
                }
            }
        } else if #[cfg(windows)] {
            unsafe extern "system" fn trap_handler(exception_info: *mut EXCEPTION_POINTERS) -> i32 {
                match (*(*exception_info).ExceptionRecord).ExceptionCode {
                    EXCEPTION_ACCESS_VIOLATION | EXCEPTION_STACK_OVERFLOW => {}
                    _ => return 0, // EXCEPTION_CONTINUE_SEARCH
                }

                if let Some(co) = Self::current() {
                    cfg_if::cfg_if! {
                        if #[cfg(target_arch = "x86_64")] {
                            let sp = (*(*exception_info).ContextRecord).Rsp;
                        } else if #[cfg(target_arch = "x86")] {
                            let sp = u64::from((*(*exception_info).ContextRecord).Esp);
                        } else {
                            compile_error!("Unsupported platform");
                        }
                    }

                    let stack_ptr_in_bounds = co.stack_ptr_in_bounds(sp);
                    let regs = co.inner.trap_handler().setup_trap_handler(move || {
                        Err(if stack_ptr_in_bounds {
                            "invalid memory reference"
                        } else {
                            "stack overflow"
                        })
                    });

                    cfg_if::cfg_if! {
                        if #[cfg(target_arch = "x86_64")] {
                            let TrapHandlerRegs { rip, rsp, rbp, rdi, rsi } = regs;
                            (*(*exception_info).ContextRecord).Rip = rip;
                            (*(*exception_info).ContextRecord).Rsp = rsp;
                            (*(*exception_info).ContextRecord).Rbp = rbp;
                            (*(*exception_info).ContextRecord).Rdi = rdi;
                            (*(*exception_info).ContextRecord).Rsi = rsi;
                        } else if #[cfg(target_arch = "x86")] {
                            let TrapHandlerRegs { eip, esp, ebp, eax, ebx, ecx, edx } = regs;
                            (*(*exception_info).ContextRecord).Eip = eip;
                            (*(*exception_info).ContextRecord).Esp = esp;
                            (*(*exception_info).ContextRecord).Ebp = ebp;
                            (*(*exception_info).ContextRecord).Eax = eax;
                            (*(*exception_info).ContextRecord).Ebx = ebx;
                            (*(*exception_info).ContextRecord).Ecx = ecx;
                            (*(*exception_info).ContextRecord).Edx = edx;
                        } else {
                            compile_error!("Unsupported platform");
                        }
                    }
                }
                // EXCEPTION_CONTINUE_EXECUTION is -1. Not to be confused with
                // ExceptionContinueExecution which has a value of 0.
                -1
            }
        }
    }

    /// handle SIGBUS and SIGSEGV
    fn setup_trap_handler() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static TRAP_HANDLER_INITED: AtomicBool = AtomicBool::new(false);
        if TRAP_HANDLER_INITED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            #[cfg(unix)]
            {
                // install SIGSEGV & SIGBUS signal handler
                let mut set = SigSet::empty();
                set.add(Signal::SIGBUS);
                set.add(Signal::SIGSEGV);
                let sa = SigAction::new(
                    SigHandler::SigAction(Self::trap_handler),
                    SaFlags::SA_ONSTACK,
                    set,
                );
                unsafe {
                    _ = sigaction(Signal::SIGBUS, &sa).expect("install SIGBUS handler failed !");
                    _ = sigaction(Signal::SIGSEGV, &sa).expect("install SIGSEGV handler failed !");
                }
            }
            #[cfg(windows)]
            assert!(
                !unsafe { AddVectoredExceptionHandler(1, Some(Self::trap_handler)).is_null() },
                "failed to add exception handler: {}",
                Error::last_os_error()
            );
        }
    }

    pub(crate) fn add_raw_listener(&mut self, listener: &'c dyn Listener<Yield, Return>) {
        self.listeners.push_back(listener);
    }

    pub(crate) fn stack_infos_ref(&self) -> &VecDeque<StackInfo> {
        unsafe {
            self.stack_infos
                .get()
                .as_ref()
                .expect("StackInfo not init !")
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub(crate) fn stack_infos_mut(&self) -> &mut VecDeque<StackInfo> {
        unsafe {
            self.stack_infos
                .get()
                .as_mut()
                .expect("StackInfo not init !")
        }
    }

    /// Grows the call stack if necessary.
    ///
    /// This function is intended to be called at manually instrumented points in a program where
    /// recursion is known to happen quite a bit. This function will check to see if we're within
    /// `red_zone` bytes of the end of the stack, and if so it will allocate a new stack of at least
    /// `stack_size` bytes.
    ///
    /// The closure `f` is guaranteed to run on a stack with at least `red_zone` bytes, and it will be
    /// run on the current stack if there's space available.
    #[inline(always)]
    #[allow(clippy::inline_always)]
    #[doc = include_str!("../../docs/en/scalable-stack.md")]
    pub fn maybe_grow_with<R, F: FnOnce() -> R>(
        red_zone: usize,
        stack_size: usize,
        callback: F,
    ) -> std::io::Result<R> {
        if let Some(co) = Self::current() {
            let remaining_stack = unsafe { co.remaining_stack() };
            if remaining_stack >= red_zone {
                return Ok(callback());
            }
            return DefaultStack::new(stack_size).map(|stack| {
                co.stack_infos_mut().push_back(StackInfo::from(&stack));
                let r = corosensei::on_stack(stack, callback);
                _ = co.stack_infos_mut().pop_back();
                r
            });
        }
        // in thread
        // if we can't guess the remaining stack (unsupported on some platforms) we immediately grow
        // the stack and then cache the new stack size (which we do know now because we allocated it).
        thread_local! {
            #[allow(clippy::missing_const_for_thread_local)]
            static STACK_INFOS: RefCell<VecDeque<StackInfo>> = const { RefCell::new(VecDeque::new()) };
        }
        if let Some(last_stack_info) = STACK_INFOS.with(|s| s.borrow().back().copied()) {
            let remaining_stack = psm::stack_pointer() as usize - last_stack_info.stack_bottom;
            if remaining_stack >= red_zone {
                return Ok(callback());
            }
        }
        DefaultStack::new(stack_size).map(|stack| {
            STACK_INFOS.with(|s| {
                s.borrow_mut().push_back(StackInfo::from(&stack));
            });
            let r = corosensei::on_stack(stack, callback);
            _ = STACK_INFOS.with(|s| s.borrow_mut().pop_back());
            r
        })
    }
}

impl<Param, Yield, Return> Drop for Coroutine<'_, Param, Yield, Return> {
    fn drop(&mut self) {
        //for test_yield case
        if self.inner.started() && !self.inner.done() {
            unsafe { self.inner.force_reset() };
            warn!("Coroutine {} is dropped without complete", self.name());
        }
    }
}

impl<Param, Yield, Return> Coroutine<'_, Param, Yield, Return>
where
    Param: 'static,
    Yield: Debug + Copy + Eq + 'static,
    Return: Debug + Copy + Eq + 'static,
{
    /// Create a new coroutine.
    ///
    ///# Errors
    /// if stack allocate failed.
    pub fn new<F>(
        name: Option<String>,
        f: F,
        stack_size: Option<usize>,
        priority: Option<c_longlong>,
    ) -> std::io::Result<Self>
    where
        F: FnOnce(&Suspender<Param, Yield>, Param) -> Return + 'static,
    {
        let stack_size = stack_size
            .unwrap_or(crate::common::constants::DEFAULT_STACK_SIZE)
            .max(crate::common::page_size());
        let stack = DefaultStack::new(stack_size)?;
        let stack_infos = UnsafeCell::new(VecDeque::from([StackInfo {
            stack_top: stack.base().get(),
            stack_bottom: stack.limit().get(),
        }]));
        let name = name.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let co_name = name.clone().leak();
        let inner = corosensei::Coroutine::with_stack(stack, move |y, p| {
            catch!(
                move || {
                    let suspender = Suspender::new(y);
                    Suspender::<Param, Yield>::init_current(&suspender);
                    let r = f(&suspender, p);
                    Suspender::<Param, Yield>::clean_current();
                    r
                },
                format!("coroutine {co_name} failed without message"),
                co_name
            )
        });
        #[allow(unused_mut)]
        let mut co = Coroutine {
            name,
            inner,
            stack_infos,
            state: Cell::new(CoroutineState::Ready),
            listeners: VecDeque::default(),
            local: CoroutineLocal::default(),
            priority,
        };
        cfg_if::cfg_if! {
            if #[cfg(all(unix, feature = "preemptive"))] {
                let type_id = std::any::TypeId::of::<()>();
                if std::any::TypeId::of::<Param>() == type_id && std::any::TypeId::of::<Yield>() == type_id {
                    co.add_listener(crate::monitor::MonitorListener);
                }
            }
        }
        Ok(co)
    }

    pub(crate) fn raw_resume(
        &mut self,
        arg: Param,
    ) -> std::io::Result<CoroutineState<Yield, Return>> {
        Self::setup_trap_handler();
        match self.inner.resume(arg) {
            CoroutineResult::Yield(y) => {
                let current = self.state();
                match current {
                    CoroutineState::Running => {
                        if Suspender::<Yield, Param>::is_cancel() {
                            self.cancel()?;
                            return Ok(CoroutineState::Cancelled);
                        }
                        let timestamp = Suspender::<Yield, Param>::timestamp();
                        self.suspend(y, timestamp)?;
                        Ok(CoroutineState::Suspend(y, timestamp))
                    }
                    CoroutineState::Syscall(y, syscall, state) => {
                        Ok(CoroutineState::Syscall(y, syscall, state))
                    }
                    _ => Err(Error::other(format!(
                        "{} unexpected state {current}",
                        self.name()
                    ))),
                }
            }
            CoroutineResult::Return(result) => {
                if let Ok(returns) = result {
                    self.complete(returns)?;
                    Ok(CoroutineState::Complete(returns))
                } else {
                    let message = result.unwrap_err();
                    self.error(message)?;
                    Ok(CoroutineState::Error(message))
                }
            }
        }
    }
}

impl<S: Stack> From<&S> for StackInfo {
    fn from(stack: &S) -> Self {
        Self {
            stack_top: stack.base().get(),
            stack_bottom: stack.limit().get(),
        }
    }
}
