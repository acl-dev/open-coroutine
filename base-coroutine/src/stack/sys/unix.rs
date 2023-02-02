// Copyright 2016 coroutine-rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::mem;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::usize;

use crate::stack::Stack;

#[cfg(any(
    target_os = "openbsd",
    target_os = "macos",
    target_os = "ios",
    target_os = "android"
))]
const MAP_STACK: libc::c_int = 0;

#[cfg(not(any(
    target_os = "openbsd",
    target_os = "macos",
    target_os = "ios",
    target_os = "android"
)))]
const MAP_STACK: libc::c_int = libc::MAP_STACK;

pub unsafe fn allocate_stack(size: usize) -> std::io::Result<Stack> {
    let mut ptr = std::ptr::null_mut();
    let result = jemalloc_sys::posix_memalign(&mut ptr, page_size(), size);
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(Stack::new(
        (ptr as usize + size) as *mut c_void,
        ptr as *mut c_void,
    ))
}

pub unsafe fn protect_stack(stack: &Stack) -> std::io::Result<Stack> {
    let page_size = page_size();
    debug_assert!(stack.len() % page_size == 0 && !stack.is_empty());
    let ret = {
        let bottom = stack.bottom() as *mut libc::c_void;
        libc::mprotect(bottom, page_size, libc::PROT_NONE)
    };
    if ret != 0 {
        Err(std::io::Error::last_os_error())
    } else {
        let bottom = (stack.bottom() as usize + page_size) as *mut c_void;
        Ok(Stack::new(stack.top(), bottom))
    }
}

pub unsafe fn deallocate_stack(ptr: *mut c_void, size: usize) {
    jemalloc_sys::sdallocx(ptr, size, 0);
}

pub fn page_size() -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);
    let mut ret = PAGE_SIZE.load(Ordering::Relaxed);

    if ret == 0 {
        unsafe {
            ret = libc::sysconf(libc::_SC_PAGESIZE) as usize;
        }

        PAGE_SIZE.store(ret, Ordering::Relaxed);
    }

    ret
}

pub fn min_stack_size() -> usize {
    // Previously libc::SIGSTKSZ has been used for this, but it proofed to be very unreliable,
    // because the resulting values varied greatly between platforms.
    page_size()
}

pub fn max_stack_size(protected: bool) -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);
    let mut ret = PAGE_SIZE.load(Ordering::Relaxed);

    if ret == 0 {
        let limit = mem::MaybeUninit::uninit();
        let mut limit = unsafe { limit.assume_init() };
        let limitret = unsafe { libc::getrlimit(libc::RLIMIT_STACK, &mut limit) };

        if limitret == 0 {
            ret = if limit.rlim_max == libc::RLIM_INFINITY
                || limit.rlim_max > (usize::MAX as libc::rlim_t)
            {
                usize::MAX
            } else {
                limit.rlim_max as usize
            };

            PAGE_SIZE.store(ret, Ordering::Relaxed);
        } else {
            ret = 1024 * 1024 * 1024;
        }
    }

    let page_size = page_size();
    let add_shift = i32::from(protected);
    let protected = page_size << add_shift;
    let size = (ret - 1) & !(page_size - 1);
    size - protected
}
