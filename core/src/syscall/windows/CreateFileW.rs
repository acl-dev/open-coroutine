use std::ffi::c_uint;
use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_CREATION_DISPOSITION, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE,
};

trait CreateFileWSyscall {
    extern "system" fn CreateFileW(
        &self,
        fn_ptr: Option<
            &extern "system" fn(
                PCWSTR,
                c_uint,
                FILE_SHARE_MODE,
                *const SECURITY_ATTRIBUTES,
                FILE_CREATION_DISPOSITION,
                FILE_FLAGS_AND_ATTRIBUTES,
                HANDLE,
            ) -> HANDLE,
        >,
        lpfilename: PCWSTR,
        dwdesiredaccess: c_uint,
        dwsharemode: FILE_SHARE_MODE,
        lpsecurityattributes: *const SECURITY_ATTRIBUTES,
        dwcreationdisposition: FILE_CREATION_DISPOSITION,
        dwflagsandattributes: FILE_FLAGS_AND_ATTRIBUTES,
        htemplatefile: HANDLE,
    ) -> HANDLE;
}

impl_syscall!(CreateFileWSyscallFacade, RawCreateFileWSyscall,
    CreateFileW(
        lpfilename: PCWSTR,
        dwdesiredaccess: c_uint,
        dwsharemode: FILE_SHARE_MODE,
        lpsecurityattributes: *const SECURITY_ATTRIBUTES,
        dwcreationdisposition: FILE_CREATION_DISPOSITION,
        dwflagsandattributes: FILE_FLAGS_AND_ATTRIBUTES,
        htemplatefile: HANDLE
    ) -> HANDLE
);

impl_facade!(CreateFileWSyscallFacade, CreateFileWSyscall,
    CreateFileW(
        lpfilename: PCWSTR,
        dwdesiredaccess: c_uint,
        dwsharemode: FILE_SHARE_MODE,
        lpsecurityattributes: *const SECURITY_ATTRIBUTES,
        dwcreationdisposition: FILE_CREATION_DISPOSITION,
        dwflagsandattributes: FILE_FLAGS_AND_ATTRIBUTES,
        htemplatefile: HANDLE
    ) -> HANDLE
);

impl_raw!(RawCreateFileWSyscall, CreateFileWSyscall, windows_sys::Win32::Storage::FileSystem,
    CreateFileW(
        lpfilename: PCWSTR,
        dwdesiredaccess: c_uint,
        dwsharemode: FILE_SHARE_MODE,
        lpsecurityattributes: *const SECURITY_ATTRIBUTES,
        dwcreationdisposition: FILE_CREATION_DISPOSITION,
        dwflagsandattributes: FILE_FLAGS_AND_ATTRIBUTES,
        htemplatefile: HANDLE
    ) -> HANDLE
);