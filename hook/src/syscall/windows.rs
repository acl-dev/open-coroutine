use std::ffi::{c_int, c_uint, c_void};
use std::io::{Error, ErrorKind};
use windows_sys::core::{PCSTR, PSTR};
use windows_sys::Win32::Foundation::{BOOL, TRUE};
use windows_sys::Win32::Networking::WinSock::{
    IPPROTO, LPWSAOVERLAPPED_COMPLETION_ROUTINE, SEND_RECV_FLAGS, SOCKADDR, SOCKET,
    WINSOCK_SHUTDOWN_HOW, WINSOCK_SOCKET_TYPE, WSABUF, WSAPROTOCOL_INFOW,
};
use windows_sys::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
use windows_sys::Win32::System::IO::OVERLAPPED;

// check https://www.rustwiki.org.cn/en/reference/introduction.html for help information
#[allow(unused_macros)]
macro_rules! impl_hook {
    ( $module_name: expr, $field_name: ident, $syscall: ident($($arg: ident : $arg_type: ty),*) -> $result: ty ) => {
        static $field_name: once_cell::sync::OnceCell<extern "system" fn($($arg_type),*) -> $result> =
            once_cell::sync::OnceCell::new();
        _ = $field_name.get_or_init(|| unsafe {
            let syscall: &str = open_coroutine_core::common::constants::Syscall::$syscall.into();
            let ptr = minhook::MinHook::create_hook_api($module_name, syscall, $syscall as _)
                .unwrap_or_else(|_| panic!("hook {syscall} failed !"));
            assert!(!ptr.is_null(), "syscall \"{syscall}\" not found !");
            std::mem::transmute(ptr)
        });
        #[allow(non_snake_case)]
        extern "system" fn $syscall($($arg: $arg_type),*) -> $result {
            let fn_ptr = $field_name.get().unwrap_or_else(|| {
                panic!(
                    "hook {} failed !",
                    open_coroutine_core::common::constants::Syscall::$syscall
                )
            });
            if $crate::hook() {
                return open_coroutine_core::syscall::$syscall(Some(fn_ptr), $($arg),*);
            }
            (fn_ptr)($($arg),*)
        }
    }
}

#[no_mangle]
#[allow(non_snake_case, clippy::missing_safety_doc)]
pub unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    call_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    // Preferably a thread should be created here instead, since as few
    // operations as possible should be performed within `DllMain`.
    if call_reason == DLL_PROCESS_ATTACH {
        // Called when the DLL is attached to the process.
        BOOL::from(attach().is_ok())
    } else if call_reason == DLL_PROCESS_DETACH {
        // Called when the DLL is detached to the process.
        BOOL::from(minhook::MinHook::disable_all_hooks().is_ok())
    } else {
        TRUE
    }
}

unsafe fn attach() -> std::io::Result<()> {
    impl_hook!("ws2_32.dll", ACCEPT, accept(
        fd: SOCKET,
        address: *mut SOCKADDR,
        address_len: *mut c_int
    ) -> SOCKET);
    impl_hook!("ws2_32.dll", IOCTLSOCKET, ioctlsocket(
        fd: SOCKET,
        cmd: c_int,
        argp: *mut c_uint
    ) -> c_int);
    impl_hook!("ws2_32.dll", LISTEN, listen(fd: SOCKET, backlog: c_int) -> c_int);
    impl_hook!("ws2_32.dll", RECV, recv(
        fd: SOCKET,
        buf: PSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS
    ) -> c_int);
    impl_hook!("ws2_32.dll", SEND, send(
        fd: SOCKET,
        buf: PCSTR,
        len: c_int,
        flags: SEND_RECV_FLAGS
    ) -> c_int);
    impl_hook!("ws2_32.dll", SHUTDOWN, shutdown(fd: SOCKET, how: WINSOCK_SHUTDOWN_HOW) -> c_int);
    impl_hook!("kernel32.dll", SLEEP, Sleep(dw_milliseconds: u32) -> ());
    impl_hook!("ws2_32.dll", SOCKET, socket(
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO
    ) -> SOCKET);
    impl_hook!("ws2_32.dll", WSARECV, WSARecv(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        lpflags : *mut c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int);
    impl_hook!("ws2_32.dll", WSASEND, WSASend(
        fd: SOCKET,
        buf: *const WSABUF,
        dwbuffercount: c_uint,
        lpnumberofbytesrecvd: *mut c_uint,
        dwflags : c_uint,
        lpoverlapped: *mut OVERLAPPED,
        lpcompletionroutine : LPWSAOVERLAPPED_COMPLETION_ROUTINE
    ) -> c_int);
    impl_hook!("ws2_32.dll", WSASOCKETW, WSASocketW(
        domain: c_int,
        ty: WINSOCK_SOCKET_TYPE,
        protocol: IPPROTO,
        lpprotocolinfo: *const WSAPROTOCOL_INFOW,
        g: c_uint,
        dw_flags: c_uint
    ) -> SOCKET);
    // Enable the hook
    minhook::MinHook::enable_all_hooks()
        .map_err(|_| Error::new(ErrorKind::Other, "init all hooks failed !"))
}
