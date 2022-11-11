use std::borrow::Borrow;
use std::collections::hash_map::{Keys, RandomState};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};
use std::os::raw::c_void;

#[repr(C)]
#[derive(Debug)]
pub struct ObjectMap<K, S = RandomState> {
    inner: HashMap<K, *mut c_void, S>,
}

impl<K, S> ObjectMap<K, S> {
    pub fn keys(&self) -> Keys<'_, K, *mut c_void> {
        self.inner.keys()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn clear(&mut self) {
        self.inner.clear()
    }

    pub fn hasher(&mut self) -> &S {
        self.inner.hasher()
    }
}

impl<K> ObjectMap<K, RandomState> {
    pub fn new() -> Self {
        ObjectMap {
            inner: HashMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        ObjectMap {
            inner: HashMap::with_capacity(capacity),
        }
    }
}

impl<K, S> ObjectMap<K, S>
where
    K: Eq + Hash,
    S: BuildHasher,
{
    pub fn with_capacity_and_hasher(capacity: usize, hasher: S) -> Self {
        ObjectMap {
            inner: HashMap::with_capacity_and_hasher(capacity, hasher),
        }
    }

    pub fn insert<V>(&mut self, k: K, v: V) -> Option<*mut c_void> {
        let ptr = Box::leak(Box::new(v));
        self.inner.insert(k, ptr as *mut _ as *mut c_void)
    }

    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<*mut c_void>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.inner.remove(k)
    }

    pub fn get<Q: ?Sized, T>(&self, k: &Q) -> Option<&T>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self.inner.get(k) {
            Some(v) => unsafe {
                Some(std::ptr::read_unaligned(
                    v as *const *mut c_void as *const &T,
                ))
            },
            None => None,
        }
    }

    pub fn get_mut<Q: ?Sized, T>(&mut self, k: &Q) -> Option<&mut T>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        match self.inner.get_mut(k) {
            Some(v) => unsafe {
                Some(std::ptr::read_unaligned(
                    v as *mut *mut c_void as *mut &mut T,
                ))
            },
            None => None,
        }
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.inner.contains_key(k)
    }
}

impl<K> Default for ObjectMap<K, RandomState> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::ObjectMap;

    #[test]
    fn test() {
        let mut map = ObjectMap::new();
        assert!(map.is_empty());
        map.insert(1, 2i32);
        map.insert(2, true);
        assert!(!map.is_empty());
        let x: &mut i32 = map.get_mut(&1).unwrap();
        assert_eq!(&mut 2, x);
        let y: &mut bool = map.get_mut(&2).unwrap();
        assert_eq!(&mut true, y);
    }
}
