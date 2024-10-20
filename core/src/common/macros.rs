/// Constructs an event at the trace level.
#[allow(unused_macros)]
#[macro_export]
macro_rules! trace {
    ($( $args:expr ),*) => {
        #[cfg(all(debug_assertions, feature = "log"))]
        tracing::trace!( $( $args ),* );
    }
}

/// Constructs an event at the info level.
#[allow(unused_macros)]
#[macro_export]
macro_rules! info {
    ($( $args:expr ),*) => {
        #[cfg(feature = "log")]
        tracing::info!( $( $args ),* );
    }
}

/// Constructs an event at the warn level.
#[allow(unused_macros)]
#[macro_export]
macro_rules! warn {
    ($( $args:expr ),*) => {
        #[cfg(feature = "log")]
        tracing::warn!( $( $args ),* );
    }
}

/// Constructs an event at the error level.
#[allow(unused_macros)]
#[macro_export]
macro_rules! error {
    ($( $args:expr ),*) => {
        #[cfg(feature = "log")]
        tracing::error!( $( $args ),* );
    }
}

/// Catch panic.
#[macro_export]
macro_rules! catch {
    ($f:expr, $msg:expr, $arg:expr) => {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe($f)).map_err(|e| {
            let message = if let Some(msg) = e.downcast_ref::<&'static str>() {
                *msg
            } else {
                $msg.leak()
            };
            $crate::error!("{} failed with error:{}", $arg, message);
            message
        })
    };
}

/// Fast impl `Display` trait for `Debug` types.
#[allow(unused_macros)]
#[macro_export]
macro_rules! impl_display_by_debug {
    ($struct_name:ident$(<$($generic1:tt $( : $trait_tt1: tt $( + $trait_tt2: tt)*)?),+>)?
        $(where $(
            $generic2:tt $( : $trait_tt3: tt $( + $trait_tt4: tt)*)?
        ),+)?
    ) => {
        impl$(<$($generic1 $( : $trait_tt1 $( + $trait_tt2)*)?),+>)? std::fmt::Display
            for $struct_name$(<$($generic1),+>)?
        where
            $($($generic2 $( : $trait_tt3 $( + $trait_tt4)*)?),+,)?
            $struct_name$(<$($generic1),+>)?: std::fmt::Debug,
        {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Debug::fmt(self, f)
            }
        }
    };
}

/// Fast impl `Current` for a type.
/// This crate use `std` cause `#![no_std]` not support `thread_local!`.
#[allow(unused_macros)]
#[macro_export]
macro_rules! impl_current_for {
    (
        $name:ident,
        $struct_name:ident$(<$($generic1:tt $( : $trait_tt1: tt $( + $trait_tt2: tt)*)?),+>)?
        $(where $(
            $generic2:tt $( : $trait_tt3: tt $( + $trait_tt4: tt)*)?
        ),+)?
    ) => {
        thread_local! {
            static $name: std::cell::RefCell<std::collections::VecDeque<*const std::ffi::c_void>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
        }

        impl$(<$($generic1 $( : $trait_tt1 $( + $trait_tt2)*)?),+>)? $struct_name$(<$($generic1),+>)?
            $(where $($generic2 $( : $trait_tt3 $( + $trait_tt4)*)?),+)?
        {
            /// Init the current.
            pub(crate) fn init_current(current: &Self) {
                $name.with(|s| {
                    s.borrow_mut()
                        .push_front(core::ptr::from_ref(current).cast::<std::ffi::c_void>());
                });
            }

            /// Get the current if has.
            #[must_use]
            #[allow(unreachable_pub)]
            pub fn current<'current>() -> Option<&'current Self> {
                $name.with(|s| {
                    s.borrow()
                        .front()
                        .map(|ptr| unsafe { &*(*ptr).cast::<Self>() })
                })
            }

            /// Clean the current.
            pub(crate) fn clean_current() {
                $name.with(|s| _ = s.borrow_mut().pop_front());
            }
        }
    };
}

/// Fast impl common traits for `Named` types.
/// Check <https://www.rustwiki.org.cn/en/reference/introduction.html> for help information.
#[macro_export]
macro_rules! impl_for_named {
    ($struct_name:ident$(<$($generic1:tt $( : $trait_tt1: tt $( + $trait_tt2: tt)*)?),+>)?
        $(where $(
            $generic2:tt $( : $trait_tt3: tt $( + $trait_tt4: tt)*)?
        ),+)?
    ) => {
        impl$(<$($generic1 $( : $trait_tt1 $( + $trait_tt2)*)?),+>)? Eq
            for $struct_name$(<$($generic1),+>)?
        {
        }

        impl$(<$($generic1 $( : $trait_tt1 $( + $trait_tt2)*)?),+>)? PartialEq<Self>
            for $struct_name$(<$($generic1),+>)?
        {
            fn eq(&self, other: &Self) -> bool {
                self.name().eq(other.name())
            }
        }

        impl$(<$($generic1 $( : $trait_tt1 $( + $trait_tt2)*)?),+>)? Ord
            for $struct_name$(<$($generic1),+>)?
        {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.name().cmp(other.name())
            }
        }

        impl$(<$($generic1 $( : $trait_tt1 $( + $trait_tt2)*)?),+>)? PartialOrd<Self>
            for $struct_name$(<$($generic1),+>)?
        {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl$(<$($generic1 $( : $trait_tt1 $( + $trait_tt2)*)?),+>)? std::hash::Hash
            for $struct_name$(<$($generic1),+>)?
        {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.name().hash(state)
            }
        }
    };
}
