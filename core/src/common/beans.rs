use dashmap::DashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Simple bean factory.
#[repr(C)]
#[derive(Debug, Default)]
pub struct BeanFactory<'b>(DashMap<&'b str, usize>);

impl BeanFactory<'_> {
    fn get_instance<'i>() -> &'i BeanFactory<'i> {
        static INSTANCE: AtomicUsize = AtomicUsize::new(0);
        let mut ret = INSTANCE.load(Ordering::Relaxed);
        if ret == 0 {
            let ptr: &'i mut BeanFactory = Box::leak(Box::default());
            ret = std::ptr::from_mut(ptr) as usize;
            INSTANCE.store(ret, Ordering::Relaxed);
        }
        unsafe { &*(ret as *mut BeanFactory) }
    }

    /// Init bean if not exists.
    pub fn init_bean<B>(bean_name: &str, bean: B) {
        let factory = Self::get_instance();
        if factory.0.get(bean_name).is_none() {
            let bean: &B = Box::leak(Box::new(bean));
            assert!(factory
                .0
                .insert(
                    Box::leak(Box::from(bean_name)),
                    std::ptr::from_ref(bean) as usize,
                )
                .is_none());
        }
    }

    /// Remove bean if exists.
    #[must_use]
    pub fn remove_bean<B>(bean_name: &str) -> Option<B> {
        Self::get_instance()
            .0
            .remove(bean_name)
            .map(|(_, ptr)| unsafe { *Box::from_raw((ptr as *mut c_void).cast::<B>()) })
    }

    /// Get the bean by name.
    #[must_use]
    pub fn get_bean<B>(bean_name: &str) -> Option<&B> {
        Self::get_instance()
            .0
            .get(bean_name)
            .map(|ptr| unsafe { &*(*ptr as *mut c_void).cast::<B>() })
    }

    /// Get the bean by name.
    ///
    /// # Safety
    /// Only one mutable reference can be held for a given bean at a time.
    #[must_use]
    pub unsafe fn get_mut_bean<B>(bean_name: &str) -> Option<&mut B> {
        Self::get_instance()
            .0
            .get_mut(bean_name)
            .map(|ptr| &mut *(*ptr as *mut c_void).cast::<B>())
    }

    /// Get the bean by name, create bean if not exists.
    #[must_use]
    pub fn get_or_default<B: Default>(bean_name: &str) -> &B {
        let factory = Self::get_instance();
        factory.0.get(bean_name).map_or_else(
            || {
                let bean: &B = Box::leak(Box::default());
                _ = factory.0.insert(
                    Box::leak(Box::from(bean_name)),
                    std::ptr::from_ref(bean) as usize,
                );
                bean
            },
            |ptr| unsafe { &*(*ptr as *mut c_void).cast::<B>() },
        )
    }

    /// Get the bean by name, create bean if not exists.
    ///
    /// # Safety
    /// Only one mutable reference can be held for a given bean at a time.
    #[must_use]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut_or_default<B: Default>(bean_name: &str) -> &mut B {
        let factory = Self::get_instance();
        factory.0.get_mut(bean_name).map_or_else(
            || {
                let bean: &mut B = Box::leak(Box::default());
                _ = factory.0.insert(
                    Box::leak(Box::from(bean_name)),
                    std::ptr::from_ref(bean) as usize,
                );
                bean
            },
            |ptr| &mut *(*ptr as *mut c_void).cast::<B>(),
        )
    }
}
