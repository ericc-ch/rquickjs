use crate::{class::ExoticHooks, qjs};
use alloc::boxed::Box;

#[derive(Debug)]
#[repr(transparent)]
pub(crate) struct ExoticMethodsHolder(*mut qjs::JSClassExoticMethods);

impl ExoticMethodsHolder {
    pub fn new(hooks: ExoticHooks) -> Self {
        Self(Box::into_raw(Box::new(qjs::JSClassExoticMethods {
            get_own_property: hooks
                .get_own_property
                .then_some(crate::class::ffi::exotic_get_own_property),
            get_own_property_names: hooks
                .get_own_property_names
                .then_some(crate::class::ffi::exotic_get_own_property_names),
            delete_property: hooks
                .delete
                .then_some(crate::class::ffi::exotic_delete_property),
            define_own_property: None, // TODO: Implement
            has_property: hooks.has.then_some(crate::class::ffi::exotic_has_property),
            set_property: hooks.set.then_some(crate::class::ffi::exotic_set_property),
            get_property: hooks.get.then_some(crate::class::ffi::exotic_get_property),
        })))
    }

    pub(crate) fn as_ptr(&self) -> *mut qjs::JSClassExoticMethods {
        self.0
    }
}

impl Drop for ExoticMethodsHolder {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.0) };
    }
}
