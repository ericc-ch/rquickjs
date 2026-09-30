//! ABI declarations for the maintained fork's realm-aware callback extensions.
//! These declarations use the target's generated opaque and value types.

use crate::{JSContext, JSValue, JSValueConst};

unsafe extern "C" {
    /// Returns a borrowed context, or null after throwing for a revoked proxy.
    /// `ctx` and the callable `function` must belong to the same live runtime.
    pub fn JS_GetFunctionRealm(ctx: *mut JSContext, function: JSValueConst) -> *mut JSContext;

    /// Returns an owned reference to the active function, or undefined outside a call.
    /// `ctx` must belong to a live runtime entered by the calling thread.
    pub fn JS_GetActiveFunctionRef(ctx: *mut JSContext) -> JSValue;

    /// Reports whether the active call uses [[Construct]].
    /// `ctx` must belong to a live runtime entered by the calling thread.
    pub fn JS_IsConstructorCall(ctx: *mut JSContext) -> bool;
}
