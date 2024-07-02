pub use nanosleep::nanosleep;
pub use sleep::sleep;
pub use usleep::usleep;

macro_rules! impl_facade {
    ( $struct_name:ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[derive(Debug, Default)]
        struct $struct_name<I: $trait_name> {
            inner: I,
        }

        impl<I: $trait_name> $trait_name for $struct_name<I> {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($($arg_type),*) -> $result>,
                $($arg: $arg_type),*
            ) -> $result {
                use $crate::constants::{Syscall, SyscallState};
                use $crate::common::{Current, Named};
                use $crate::scheduler::SchedulableCoroutine;

                let syscall = Syscall::$syscall;
                $crate::info!("hook syscall {}", syscall);
                if let Some(co) = SchedulableCoroutine::current() {
                    let new_state = SyscallState::Executing;
                    if co.syscall((), syscall, new_state).is_err() {
                        $crate::error!("{} change to syscall {} {} failed !",
                            co.get_name(), syscall, new_state);
                    }
                }
                let r = self.inner.$syscall(fn_ptr, $($arg, )*);
                if let Some(co) = SchedulableCoroutine::current() {
                    if co.running().is_err() {
                        $crate::error!("{} change to running state failed !", co.get_name());
                    }
                }
                r
            }
        }
    }
}

#[allow(unused_macros)]
macro_rules! impl_raw {
    ( $struct_name: ident, $trait_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        #[derive(Debug, Copy, Clone, Default)]
        struct $struct_name {}

        impl $trait_name for $struct_name {
            extern "C" fn $syscall(
                &self,
                fn_ptr: Option<&extern "C" fn($($arg_type),*) -> $result>,
                $($arg: $arg_type),*
            ) -> $result {
                if let Some(f) = fn_ptr {
                    (f)($($arg),*)
                } else {
                    unsafe { libc::$syscall($($arg),*) }
                }
            }
        }
    }
}

mod nanosleep;
mod sleep;
mod usleep;
