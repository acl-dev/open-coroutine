use dashmap::DashMap;
use std::ffi::c_void;

/// A struct for coroutines handles local args.
#[repr(C)]
#[derive(Debug, Default)]
pub struct CoroutineLocal(DashMap<&'static str, usize>);

#[allow(missing_docs, box_pointers)]
impl CoroutineLocal {
    #[must_use]
    pub fn put<V>(&self, key: &str, val: V) -> Option<V> {
        let k: &str = Box::leak(Box::from(key));
        let v = Box::leak(Box::new(val));
        self.0
            .insert(k, std::ptr::from_mut::<V>(v) as usize)
            .map(|ptr| unsafe { *Box::from_raw((ptr as *mut c_void).cast::<V>()) })
    }

    #[must_use]
    pub fn get<V>(&self, key: &str) -> Option<&V> {
        let k: &str = Box::leak(Box::from(key));
        self.0
            .get(k)
            .map(|ptr| unsafe { &*(*ptr as *mut c_void).cast::<V>() })
    }

    #[must_use]
    pub fn get_mut<V>(&self, key: &str) -> Option<&mut V> {
        let k: &str = Box::leak(Box::from(key));
        self.0
            .get(k)
            .map(|ptr| unsafe { &mut *(*ptr as *mut c_void).cast::<V>() })
    }

    #[must_use]
    pub fn remove<V>(&self, key: &str) -> Option<V> {
        let k: &str = Box::leak(Box::from(key));
        self.0
            .remove(k)
            .map(|ptr| unsafe { *Box::from_raw((ptr.1 as *mut c_void).cast::<V>()) })
    }
}

#[allow(missing_docs)]
pub trait HasCoroutineLocal {
    fn local(&self) -> &CoroutineLocal;

    fn put<V>(&self, key: &str, val: V) -> Option<V> {
        self.local().put(key, val)
    }

    fn get<V>(&self, key: &str) -> Option<&V> {
        self.local().get(key)
    }

    fn get_mut<V>(&self, key: &str) -> Option<&mut V> {
        self.local().get_mut(key)
    }

    fn remove<V>(&self, key: &str) -> Option<V> {
        self.local().remove(key)
    }
}

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
