//! Realm-aware callbacks whose Rust state cannot borrow JavaScript values.

use alloc::boxed::Box;
use core::ptr;

use crate::{qjs, Ctx, Function, Result, Value};

use super::Params;

/// A callback with copyable Rust state and no borrowed JavaScript captures.
///
/// Unlike signature-derived callbacks, this callback receives the original
/// argument count and performs its own validation. The callback runs in the
/// realm in which its function was created. When `Params::is_constructor()` is
/// true, `Params::this()` is `new.target`; no prototype lookup is performed by
/// the wrapper. Returned values and errors use that same runtime.
///
/// Implementations must not keep values borrowed from `params` after returning.
/// The state is not traversed by the JavaScript collector. `Copy` excludes
/// owned JavaScript references and user destructors during collection; use
/// `Function::new` for callbacks that need resource-owning Rust state instead.
/// See [`Function::new_native`] for an implementation example.
pub trait NativeFunc: 'static + Copy + Send + Sync {
    /// Handle one call using the function's home context and original arguments.
    /// Return a value from that runtime, or an error to throw in JavaScript.
    /// A Rust panic resumes when execution returns to the Rust caller.
    fn call<'js>(&self, params: Params<'_, 'js>) -> Result<Value<'js>>;
}

impl<'js> Function<'js> {
    /// Create a realm-aware native callback with a default length of zero.
    ///
    /// The callback owns its Rust state until the function is collected. It can
    /// be made constructible with `with_constructor(true)`; constructor calls
    /// enter it with `new.target` without first inspecting its prototype.
    ///
    /// Returns an error if the engine cannot allocate the function. Callback
    /// ownership is transferred only after successful engine registration.
    ///
    /// ```
    /// use rquickjs_core::{Context, Function, Result, Runtime, Value};
    /// use rquickjs_core::function::{NativeFunc, Params};
    ///
    /// #[derive(Clone, Copy)]
    /// struct Echo;
    /// impl NativeFunc for Echo {
    ///     fn call<'js>(&self, params: Params<'_, 'js>) -> Result<Value<'js>> {
    ///         Ok(params.arg(0).unwrap_or_else(|| Value::new_undefined(params.ctx().clone())))
    ///     }
    /// }
    /// # fn main() -> Result<()> {
    /// let runtime = Runtime::new()?;
    /// let context = Context::full(&runtime)?;
    /// context.with(|ctx| -> Result<()> {
    ///     let echo = Function::new_native(ctx, Echo)?;
    ///     assert_eq!(echo.call::<_, String>(("hello",))?, "hello");
    ///     Ok(())
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_native<F: NativeFunc>(ctx: Ctx<'js>, callback: F) -> Result<Self> {
        let callback = Box::into_raw(Box::new(callback));
        // SAFETY: ctx is a live, locked runtime context. opaque points to the
        // boxed F, and both function pointers are monomorphized for that F.
        // JS_NewCClosure adopts opaque only on success, after all allocations.
        let value = unsafe {
            qjs::JS_NewCClosure(
                ctx.as_ptr(),
                Some(call::<F>),
                ptr::null(),
                Some(finalize::<F>),
                0,
                0,
                callback.cast(),
            )
        };
        // SAFETY: value is the engine return tag and can be inspected without
        // taking ownership. Failed registration leaves the raw Box with us;
        // reclaim it before handle_exception can resume a stored Rust panic.
        unsafe {
            if qjs::JS_IsException(value) {
                drop(Box::from_raw(callback));
            }
        }
        // SAFETY: value is owned by this context; exception tags hold no refs.
        let value = unsafe { ctx.handle_exception(value)? };
        // SAFETY: value is the owned callable object returned by JS_NewCClosure.
        Ok(Self(unsafe { crate::Object::from_js_value(ctx, value) }))
    }

    /// Return the function's realm, including through bound functions and proxies.
    ///
    /// The returned context has its own reference and shares this function's
    /// runtime and scoped lifetime. A revoked proxy throws a TypeError.
    ///
    /// ```
    /// # use rquickjs_core::{Context, Function, Result, Runtime};
    /// # fn main() -> Result<()> {
    /// # let runtime = Runtime::new()?;
    /// # let context = Context::full(&runtime)?;
    /// context.with(|ctx| -> Result<()> {
    ///     let function: Function = ctx.eval("() => 42")?;
    ///     assert_eq!(function.realm()?.as_raw(), ctx.as_raw());
    ///     Ok(())
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn realm(&self) -> Result<Ctx<'js>> {
        // SAFETY: self is a callable object in its live context's runtime.
        // QuickJS returns a borrowed context kept alive by that callable.
        let realm = unsafe { qjs::JS_GetFunctionRealm(self.ctx().as_ptr(), self.as_raw()) };
        if realm.is_null() {
            return Err(crate::Error::Exception);
        }
        // SAFETY: the non-null result is live and in the same locked runtime;
        // from_ptr takes its own context reference before self can be released.
        Ok(unsafe { Ctx::from_ptr(realm) })
    }
}

unsafe extern "C" fn call<F: NativeFunc>(
    ctx: *mut qjs::JSContext,
    this: qjs::JSValue,
    argc: qjs::c_int,
    argv: *mut qjs::JSValue,
    _magic: qjs::c_int,
    opaque: *mut qjs::c_void,
) -> qjs::JSValue {
    // SAFETY: QuickJS invokes this callback with its live home-realm context,
    // valid this and argument values, and the F registered by new_native.
    let context = unsafe { Ctx::from_ptr(ctx) };
    context.handle_panic(|| {
        // SAFETY: the engine's active frame owns the function throughout this
        // call; JS_GetActiveFunctionRef adds the reference owned by function.
        let function =
            unsafe { Value::from_js_value(context.clone(), qjs::JS_GetActiveFunctionRef(ctx)) };
        // SAFETY: the active frame belongs to this callback, so its constructor
        // flag describes this/new.target rather than an enclosing caller.
        let flags = if unsafe { qjs::JS_IsConstructorCall(ctx) } {
            qjs::JS_CALL_FLAG_CONSTRUCTOR as qjs::c_int
        } else {
            0
        };
        // SAFETY: QuickJS guarantees argc nonnegative and argv readable for
        // argc values until return. Params handles null and unaligned argv.
        // function and this remain live for the whole borrowed Params call.
        let params =
            unsafe { Params::from_ffi_class(ctx, function.as_raw(), this, argc, argv, flags) };
        // SAFETY: opaque is the boxed F passed to this exact monomorphization.
        // The function is live during invocation, so finalization cannot race.
        let callback = unsafe { &*opaque.cast::<F>() };
        match callback.call(params) {
            Ok(value) => value.into_js_value(),
            Err(error) => error.throw(&context),
        }
    })
}

unsafe extern "C" fn finalize<F: NativeFunc>(opaque: *mut qjs::c_void) {
    // SAFETY: successful registration transfers one Box<F> to QuickJS, which
    // calls this finalizer exactly once after all invocations have finished.
    drop(unsafe { Box::from_raw(opaque.cast::<F>()) });
}

#[cfg(test)]
mod tests {
    use crate::{Context, Persistent, Runtime};

    use super::*;

    #[derive(Clone, Copy)]
    struct Callback(i32);

    impl NativeFunc for Callback {
        fn call<'js>(&self, params: Params<'_, 'js>) -> Result<Value<'js>> {
            params.ctx().run_gc();
            Ok(params
                .arg(0)
                .unwrap_or_else(|| Value::new_int(params.ctx().clone(), self.0)))
        }
    }

    #[test]
    fn native_state_and_arguments_survive_collection_during_calls() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let function = Function::new_native(ctx.clone(), Callback(37)).unwrap();
            let result: String = function.call(("payload",)).unwrap();
            assert_eq!(result, "payload");
            let state: i32 = function.call(()).unwrap();
            assert_eq!(state, 37);
            drop(function);
            ctx.run_gc();
        });
        drop(context);
        runtime.run_gc();
    }

    #[test]
    fn native_realm_cycle_is_collected_after_context_teardown() {
        let runtime = Runtime::new().unwrap();
        let initial = runtime.memory_usage().obj_count;
        {
            let context = Context::full(&runtime).unwrap();
            context.with(|ctx| {
                let function = Function::new_native(ctx.clone(), Callback(37)).unwrap();
                function.set("self", function.clone()).unwrap();
                ctx.globals().set("callback", function).unwrap();
            });
        }
        runtime.run_gc();
        assert_eq!(runtime.memory_usage().obj_count, initial);
    }

    #[test]
    fn native_registration_reports_allocation_failures_without_partial_success() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        let mut failures = 0;
        let mut successes = 0;
        for extra in (0..2048).step_by(8) {
            let used = usize::try_from(runtime.memory_usage().malloc_size).unwrap();
            runtime.set_memory_limit(used + extra);
            let saved = context.with(
                |ctx| match Function::new_native(ctx.clone(), Callback(37)) {
                    Ok(function) => {
                        assert!(!ctx.has_exception());
                        successes += 1;
                        Some(Persistent::save(&ctx, function))
                    }
                    Err(_) => {
                        let _exception = ctx.catch();
                        failures += 1;
                        None
                    }
                },
            );
            runtime.set_memory_limit(usize::MAX);
            if let Some(saved) = saved {
                context.with(|ctx| {
                    let function = saved.restore(&ctx).unwrap();
                    assert_eq!(function.get::<_, String>("name").unwrap(), "");
                    assert_eq!(function.get::<_, i32>("length").unwrap(), 0);
                });
            }
            runtime.run_gc();
        }
        assert!(failures > 0);
        assert!(successes > 0);
    }

    #[derive(Clone, Copy)]
    struct Recursive;

    impl NativeFunc for Recursive {
        fn call<'js>(&self, params: Params<'_, 'js>) -> Result<Value<'js>> {
            let function = params.function().into_function().unwrap();
            function.call(())
        }
    }

    #[test]
    fn native_only_recursion_hits_the_engine_stack_limit() {
        let runtime = Runtime::new().unwrap();
        runtime.set_max_stack_size(128 * 1024);
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let function = Function::new_native(ctx.clone(), Recursive).unwrap();
            assert!(function.call::<_, ()>(()).is_err());
            let exception = ctx.catch().into_object().unwrap();
            assert_eq!(
                exception.get::<_, String>("message").unwrap(),
                "Maximum call stack size exceeded"
            );
        });
    }

    #[derive(Clone, Copy)]
    struct Panics;

    impl NativeFunc for Panics {
        fn call<'js>(&self, _params: Params<'_, 'js>) -> Result<Value<'js>> {
            panic!("native callback panic")
        }
    }

    #[test]
    fn native_callback_panics_resume_on_the_rust_side() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            context.with(|ctx| {
                let function = Function::new_native(ctx, Panics).unwrap();
                let _result = function.call::<_, ()>(());
            });
        }))
        .unwrap_err();
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"native callback panic"));
    }

    #[test]
    fn failed_registration_releases_state_before_resuming_a_stored_panic() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        context.with(|ctx| {
            let function = Function::new_native(ctx.clone(), Panics).unwrap();
            ctx.globals().set("panics", function).unwrap();
            ctx.eval::<(), _>("try { panics(); } catch (_) {}").unwrap();
        });
        runtime.set_memory_limit(1);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            context.with(|ctx| {
                let _registration = Function::new_native(ctx, Callback(37));
            });
        }))
        .unwrap_err();
        runtime.set_memory_limit(usize::MAX);
        context.with(|ctx| {
            let _exception = ctx.catch();
        });
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"native callback panic"));
    }
}
