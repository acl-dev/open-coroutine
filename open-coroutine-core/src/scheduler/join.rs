use crate::common::JoinHandle;
use crate::scheduler::SchedulerImpl;
use std::ffi::{c_char, CStr, CString};
use std::io::{Error, ErrorKind};

#[allow(missing_docs)]
#[repr(C)]
#[derive(Debug)]
pub struct JoinHandleImpl<'p>(*const SchedulerImpl<'p>, *const c_char);

impl<'p> JoinHandle<SchedulerImpl<'p>> for JoinHandleImpl<'p> {
    #[allow(box_pointers)]
    fn new(pool: *const SchedulerImpl<'p>, name: &str) -> Self {
        let boxed: &'static mut CString = Box::leak(Box::from(
            CString::new(name).expect("init JoinHandle failed!"),
        ));
        let cstr: &'static CStr = boxed.as_c_str();
        JoinHandleImpl(pool, cstr.as_ptr())
    }

    fn get_name(&self) -> std::io::Result<&str> {
        unsafe { CStr::from_ptr(self.1) }
            .to_str()
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "Invalid task name"))
    }

    fn timeout_at_join(&self, _timeout_time: u64) -> std::io::Result<Result<Option<usize>, &str>> {
        todo!()
    }
}
