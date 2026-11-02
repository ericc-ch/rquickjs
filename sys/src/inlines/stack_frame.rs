// The stack-frame reader, declared by hand.
//
// It is declared here rather than in the generated bindings so the per-target
// files do not have to be regenerated for it: the C declaration lives in
// `quickjs.h`, and the layout below must match it.

/// One frame of the running JS stack, filled by `JS_GetStackFrameLocation`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct JSStackFrameLocation {
    /// The script the frame runs, or `JS_ATOM_NULL` when it has none. Owned by
    /// the caller once filled.
    pub filename: JSAtom,
    /// 1-based, or -1 when unknown.
    pub line_num: c_int,
    /// 1-based, or -1 when unknown.
    pub col_num: c_int,
    /// 1 for a C function frame, which runs no bytecode.
    pub native: c_int,
}

unsafe extern "C" {
    /// Fills `location` from the frame `n_stack_levels` above the innermost one
    /// (0 is the function currently running) and returns 1, or returns 0 when
    /// there is no such frame.
    pub fn JS_GetStackFrameLocation(
        ctx: *mut JSContext,
        n_stack_levels: c_int,
        location: *mut JSStackFrameLocation,
    ) -> c_int;
}
