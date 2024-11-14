use crate::impl_display_by_debug;
use dashmap::DashMap;
use std::ffi::c_void;
use std::fmt::Debug;

/// todo provide macro like [`std::thread_local`]

/// A struct for coroutines handles local args.
#[repr(C)]
#[derive(Debug, Default)]
pub struct CoroutineLocal<'c>(DashMap<&'c str, usize>);

#[allow(clippy::must_use_candidate)]
impl<'c> CoroutineLocal<'c> {
    /// Put a value into the coroutine local.
    pub fn put<V>(&self, key: &'c str, val: V) -> Option<V> {
        let v = Box::leak(Box::new(val));
        self.0
            .insert(key, std::ptr::from_mut(v) as usize)
            .map(|ptr| unsafe { *Box::from_raw((ptr as *mut c_void).cast::<V>()) })
    }

    /// Get a value ref from the coroutine local.
    pub fn get<V>(&self, key: &'c str) -> Option<&V> {
        self.0
            .get(key)
            .map(|ptr| unsafe { &*(*ptr as *mut c_void).cast::<V>() })
    }

    /// Get a mut value ref from the coroutine local.
    pub fn get_mut<V>(&self, key: &'c str) -> Option<&mut V> {
        self.0
            .get(key)
            .map(|ptr| unsafe { &mut *(*ptr as *mut c_void).cast::<V>() })
    }

    /// Remove a key from the coroutine local.
    pub fn remove<V>(&self, key: &'c str) -> Option<V> {
        self.0
            .remove(key)
            .map(|ptr| unsafe { *Box::from_raw((ptr.1 as *mut c_void).cast::<V>()) })
    }
}

impl_display_by_debug!(CoroutineLocal<'c>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local() {
        let local = CoroutineLocal::default();
        assert!(local.put("1", 1).is_none());
        assert_eq!(Some(1), local.put("1", 2));
        assert_eq!(2, *local.get("1").unwrap());
        *local.get_mut("1").unwrap() = 3;
        assert_eq!(Some(3), local.remove("1"));
    }
}
