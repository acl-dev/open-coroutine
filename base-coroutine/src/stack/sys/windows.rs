// Copyright 2016 coroutine-rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::io;
use std::mem;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::usize;

use crate::stack::Stack;

pub unsafe fn allocate_stack(size: usize) -> io::Result<Stack> {
    const NULL: winapi::LPVOID = 0 as winapi::LPVOID;
    const PROT: winapi::DWORD = winapi::PAGE_READWRITE;
    const TYPE: winapi::DWORD = winapi::MEM_COMMIT | winapi::MEM_RESERVE;

    let ptr = kernel32::VirtualAlloc(NULL, size as winapi::SIZE_T, TYPE, PROT);

    if ptr == NULL {
        Err(io::Error::last_os_error())
    } else {
        Ok(Stack::new(
            (ptr as usize + size) as *mut c_void,
            ptr as *mut c_void,
        ))
    }
}

pub unsafe fn protect_stack(stack: &Stack) -> io::Result<Stack> {
    const TYPE: winapi::DWORD = winapi::PAGE_READWRITE | winapi::PAGE_GUARD;

    let page_size = page_size();
    let mut old_prot: winapi::DWORD = 0;

    debug_assert!(stack.len() % page_size == 0 && stack.len() != 0);

    let ret = {
        let page_size = page_size as winapi::SIZE_T;
        kernel32::VirtualProtect(stack.bottom(), page_size, TYPE, &mut old_prot)
    };

    if ret == 0 {
        Err(io::Error::last_os_error())
    } else {
        let bottom = (stack.bottom() as usize + page_size) as *mut c_void;
        Ok(Stack::new(stack.top(), bottom))
    }
}

pub unsafe fn deallocate_stack(ptr: *mut c_void, _: usize) {
    kernel32::VirtualFree(ptr as winapi::LPVOID, 0, winapi::MEM_RELEASE);
}

pub fn page_size() -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);

    let mut ret = PAGE_SIZE.load(Ordering::Relaxed);

    if ret == 0 {
        ret = unsafe {
            let mut info = mem::zeroed();
            kernel32::GetSystemInfo(&mut info);
            info.dwPageSize as usize
        };

        PAGE_SIZE.store(ret, Ordering::Relaxed);
    }

    ret
}

// Windows does not seem to provide a stack limit API
pub fn min_stack_size() -> usize {
    page_size()
}

// Windows does not seem to provide a stack limit API
pub fn max_stack_size(protected: bool) -> usize {
    let page_size = page_size();
    let add_shift = i32::from(protected);
    let protected = page_size << add_shift;
    let size = (usize::MAX - 1) & !(page_size - 1);
    size - protected
}
