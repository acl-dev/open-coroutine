use std::ffi::c_longlong;
use once_cell::sync::Lazy;
use windows_sys::Win32::Foundation::{BOOL, HANDLE};
use windows_sys::Win32::Storage::FileSystem::SET_FILE_POINTER_MOVE_METHOD;

#[must_use]
pub extern "system" fn SetFilePointerEx(
    fn_ptr: Option<&extern "system" fn(HANDLE, c_longlong, *mut c_longlong, SET_FILE_POINTER_MOVE_METHOD) -> BOOL>,
    hfile: HANDLE,
    lidistancetomove: c_longlong,
    lpnewfilepointer: *mut c_longlong,
    dwmovemethod: SET_FILE_POINTER_MOVE_METHOD
) -> BOOL {
    static CHAIN: Lazy<SetFilePointerExSyscallFacade<RawSetFilePointerExSyscall>> =
        Lazy::new(Default::default);
    CHAIN.SetFilePointerEx(fn_ptr, hfile, lidistancetomove, lpnewfilepointer, dwmovemethod)
}

trait SetFilePointerExSyscall {
    extern "system" fn SetFilePointerEx(
        &self,
        fn_ptr: Option<&extern "system" fn(HANDLE, c_longlong, *mut c_longlong, SET_FILE_POINTER_MOVE_METHOD) -> BOOL>,
        hfile: HANDLE,
        lidistancetomove: c_longlong,
        lpnewfilepointer: *mut c_longlong,
        dwmovemethod: SET_FILE_POINTER_MOVE_METHOD
    ) -> BOOL;
}

impl_facade!(SetFilePointerExSyscallFacade, SetFilePointerExSyscall,
    SetFilePointerEx(
        hfile: HANDLE,
        lidistancetomove: c_longlong,
        lpnewfilepointer: *mut c_longlong,
        dwmovemethod: SET_FILE_POINTER_MOVE_METHOD
    ) -> BOOL
);

impl_raw!(RawSetFilePointerExSyscall, SetFilePointerExSyscall, windows_sys::Win32::Storage::FileSystem,
    SetFilePointerEx(
        hfile: HANDLE,
        lidistancetomove: c_longlong,
        lpnewfilepointer: *mut c_longlong,
        dwmovemethod: SET_FILE_POINTER_MOVE_METHOD
    ) -> BOOL
);