macro_rules! syscall_mod {
    ($($mod_name: ident);*) => {
        $(
            pub use $mod_name::$mod_name;
            mod $mod_name;
        )*
    }
}

#[cfg(unix)]
pub use unix::*;

#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use windows::*;

#[allow(non_snake_case)]
#[cfg(windows)]
mod windows;
