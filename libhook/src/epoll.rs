#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct epoll_data {
    pub ptr: *mut libc::c_void,
    pub fd: libc::c_int,
    pub u32: u32,
    pub u64: u64,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct epoll_event {
    pub events: u32,      /* Epoll events */
    pub data: epoll_data, /* User data variable */
}

extern "C" {
    pub fn epoll_pwait(
        epfd: libc::c_int,
        events: *mut epoll_event,
        maxevents: libc::c_int,
        timeout: libc::c_int,
        sigmask: *const libc::sigset_t,
    ) -> libc::c_int;

    pub fn epoll_wait(
        epfd: libc::c_int,
        events: *mut epoll_event,
        maxevents: libc::c_int,
        timeout: libc::c_int,
    ) -> libc::c_int;

    pub fn epoll_ctl(
        epfd: libc::c_int,
        op: libc::c_int,
        fd: libc::c_int,
        event: *mut epoll_event,
    ) -> libc::c_int;
}
