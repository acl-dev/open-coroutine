use std::ffi::c_uint;
use once_cell::sync::Lazy;
use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_CREATION_DISPOSITION, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE,
};

#[must_use]
pub extern "system" fn CreateFileW(
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
) -> HANDLE {
    static CHAIN: Lazy<CreateFileWSyscallFacade<RawCreateFileWSyscall>> =
        Lazy::new(Default::default);
    CHAIN.CreateFileW(
        fn_ptr,
        lpfilename,
        dwdesiredaccess,
        dwsharemode,
        lpsecurityattributes,
        dwcreationdisposition,
        dwflagsandattributes,
        htemplatefile
    )
}

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