use std::ffi::c_longlong;
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::SET_FILE_POINTER_MOVE_METHOD;

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

impl_syscall!(SetFilePointerExSyscallFacade, RawSetFilePointerExSyscall,
    SetFilePointerEx(
        hfile: HANDLE,
        lidistancetomove: c_longlong,
        lpnewfilepointer: *mut c_longlong,
        dwmovemethod: SET_FILE_POINTER_MOVE_METHOD
    ) -> BOOL
);

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