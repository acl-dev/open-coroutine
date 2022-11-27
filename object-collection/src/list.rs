use std::collections::VecDeque;
use std::os::raw::c_void;
use std::ptr;

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct ObjectList {
    inner: VecDeque<*mut c_void>,
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

    pub fn remove_raw(&mut self, val: *mut c_void) -> Option<*mut c_void> {
        let index = self
            .inner
            .binary_search_by(|x| x.cmp(&val))
            .unwrap_or_else(|x| x);
        self.inner.remove(index)
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
    use std::ffi::c_void;
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

        unsafe {
            let b = list.pop_back_raw().unwrap() as *mut _ as *mut bool;
            assert_eq!(true, *b);
            let n = list.pop_back_raw().unwrap() as *mut _ as *mut i32;
            assert_eq!(1, *n);
        }
    }

    #[test]
    fn test_remove_raw() {
        let mut list = ObjectList::new();
        list.push_back_raw(1 as *mut c_void);
        list.push_back_raw(2 as *mut c_void);
        list.push_back_raw(3 as *mut c_void);
        list.remove_raw(2 as *mut c_void);
        assert_eq!(1 as *mut c_void, list.pop_front_raw().unwrap());
        assert_eq!(3 as *mut c_void, list.pop_back_raw().unwrap());
    }
}
