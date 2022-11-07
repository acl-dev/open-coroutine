use crossbeam_deque::{Steal, Worker};
use std::collections::VecDeque;
use std::os::raw::c_void;
use std::ptr;

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectList {
    inner: VecDeque<*mut c_void>,
}

fn convert<T>(pointer: *mut c_void) -> Option<T> {
    unsafe {
        let node = Box::from_raw(pointer as *mut T);
        Some(*node)
    }
}

impl ObjectList {
    pub fn new() -> Self {
        ObjectList {
            inner: VecDeque::new(),
        }
    }

    pub fn front<T>(&mut self) -> Option<&T> {
        match self.inner.front() {
            Some(value) => unsafe {
                let result = ptr::read_unaligned(value) as *mut T;
                Some(&*result)
            },
            None => None,
        }
    }

    pub fn front_mut<T>(&mut self) -> Option<&mut T> {
        match self.inner.front_mut() {
            Some(value) => unsafe {
                let result = ptr::read_unaligned(value) as *mut T;
                Some(&mut *result)
            },
            None => None,
        }
    }

    pub fn front_mut_raw(&mut self) -> Option<*mut c_void> {
        self.inner
            .front_mut()
            .map(|value| unsafe { ptr::read_unaligned(value) })
    }

    pub fn push_front<T>(&mut self, element: T) {
        let ptr = Box::leak(Box::new(element));
        self.inner.push_front(ptr as *mut _ as *mut c_void);
    }

    pub fn push_front_raw(&mut self, ptr: *mut c_void) {
        self.inner.push_front(ptr);
    }

    pub fn pop_front<T>(&mut self) -> Option<T> {
        match self.inner.pop_front() {
            Some(pointer) => convert(pointer),
            None => None,
        }
    }

    /// 如果是闭包，还是要获取裸指针再手动转换，不然类型有问题
    pub fn pop_front_raw(&mut self) -> Option<*mut c_void> {
        self.inner.pop_front()
    }

    pub fn back<T>(&mut self) -> Option<&T> {
        match self.inner.back() {
            Some(value) => unsafe {
                let result = ptr::read_unaligned(value) as *mut T;
                Some(&*result)
            },
            None => None,
        }
    }

    pub fn back_mut<T>(&mut self) -> Option<&mut T> {
        match self.inner.back_mut() {
            Some(value) => unsafe {
                let result = ptr::read_unaligned(value) as *mut T;
                Some(&mut *result)
            },
            None => None,
        }
    }

    pub fn back_mut_raw(&mut self) -> Option<*mut c_void> {
        self.inner
            .back_mut()
            .map(|value| unsafe { ptr::read_unaligned(value) })
    }

    pub fn push_back<T>(&mut self, element: T) {
        let ptr = Box::leak(Box::new(element));
        self.inner.push_back(ptr as *mut _ as *mut c_void);
    }

    pub fn push_back_raw(&mut self, ptr: *mut c_void) {
        self.inner.push_back(ptr);
    }

    pub fn pop_back<T>(&mut self) -> Option<T> {
        match self.inner.pop_back() {
            Some(pointer) => convert(pointer),
            None => None,
        }
    }

    /// 如果是闭包，还是要获取裸指针再手动转换，不然类型有问题
    pub fn pop_back_raw(&mut self) -> Option<*mut c_void> {
        self.inner.pop_back()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn get<T>(&self, index: usize) -> Option<&T> {
        match self.inner.get(index) {
            Some(val) => unsafe {
                let result = ptr::read_unaligned(val) as *mut T;
                Some(&*result)
            },
            None => None,
        }
    }

    pub fn get_mut<T>(&mut self, index: usize) -> Option<&mut T> {
        match self.inner.get_mut(index) {
            Some(val) => unsafe {
                let result = ptr::read_unaligned(val) as *mut T;
                Some(&mut *result)
            },
            None => None,
        }
    }

    pub fn get_mut_raw(&mut self, index: usize) -> Option<*mut c_void> {
        self.inner
            .get_mut(index)
            .map(|pointer| unsafe { ptr::read_unaligned(pointer) })
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn move_front_to_back(&mut self) {
        if let Some(pointer) = self.inner.pop_front() {
            self.inner.push_back(pointer)
        }
    }
}

impl Default for ObjectList {
    fn default() -> Self {
        Self::new()
    }
}

impl AsRef<ObjectList> for ObjectList {
    fn as_ref(&self) -> &ObjectList {
        self
    }
}

impl AsMut<ObjectList> for ObjectList {
    fn as_mut(&mut self) -> &mut ObjectList {
        &mut *self
    }
}

#[cfg(test)]
mod tests {
    use crate::ObjectList;

    #[test]
    fn test() {
        let mut list = ObjectList::new();
        assert!(list.is_empty());
        list.push_back(1);
        assert_eq!(&1, list.front().unwrap());
        assert_eq!(&1, list.front().unwrap());
        assert!(!list.is_empty());
        list.push_back(true);
        assert_eq!(&true, list.back().unwrap());
        assert_eq!(&true, list.back().unwrap());

        assert_eq!(&1, list.get(0).unwrap());
        assert_eq!(&1, list.get(0).unwrap());
        assert_eq!(&true, list.get_mut(1).unwrap());
        assert_eq!(&true, list.get_mut(1).unwrap());

        let b: bool = list.pop_back().unwrap();
        assert_eq!(true, b);
        let n: i32 = list.pop_back().unwrap();
        assert_eq!(1, n);
    }
}
