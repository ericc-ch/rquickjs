use crate::{qjs, Ctx, Error, Result, StdString, Value};
use alloc::vec::Vec;
use core::{ffi::c_char, mem, ptr::NonNull, slice, str};

/// Rust representation of a JavaScript string.
#[derive(Debug, Clone, PartialEq, Hash)]
#[repr(transparent)]
pub struct String<'js>(pub(crate) Value<'js>);

impl<'js> String<'js> {
    /// Convert the JavaScript string to a Rust string.
    pub fn to_string(&self) -> Result<StdString> {
        let mut len = mem::MaybeUninit::uninit();
        let ptr = unsafe {
            qjs::JS_ToCStringLen(self.0.ctx.as_ptr(), len.as_mut_ptr(), self.0.as_js_value())
        };
        if ptr.is_null() {
            // Might not ever happen but I am not 100% sure
            // so just incase check it.
            return Err(Error::Unknown);
        }
        let len = unsafe { len.assume_init() };
        let bytes: &[u8] = unsafe { slice::from_raw_parts(ptr as _, len as _) };
        let result = str::from_utf8(bytes).map(|s| s.into());
        unsafe { qjs::JS_FreeCString(self.0.ctx.as_ptr(), ptr) };
        Ok(result?)
    }

    /// Convert the Javascript string to a Javascript C string.
    pub fn to_cstring(self) -> Result<CString<'js>> {
        CString::from_string(self)
    }

    /// Create a new JavaScript string from an Rust string.
    pub fn from_str(ctx: Ctx<'js>, s: &str) -> Result<Self> {
        let len = s.len();
        let ptr = s.as_ptr();
        Ok(unsafe {
            let js_val = qjs::JS_NewStringLen(ctx.as_ptr(), ptr as _, len as _);
            let js_val = ctx.handle_exception(js_val)?;
            String::from_js_value(ctx, js_val)
        })
    }

    /// Create a new JavaScript string from UTF-16 code units.
    ///
    /// Unpaired surrogates are preserved, unlike `from_str` which cannot
    /// represent them.
    pub fn from_utf16(ctx: Ctx<'js>, units: &[u16]) -> Result<Self> {
        Ok(unsafe {
            let js_val = qjs::JS_NewStringUTF16(ctx.as_ptr(), units.as_ptr(), units.len() as _);
            let js_val = ctx.handle_exception(js_val)?;
            String::from_js_value(ctx, js_val)
        })
    }

    /// Convert the JavaScript string to its UTF-16 code units.
    ///
    /// Unpaired surrogates are preserved, unlike `to_string` which maps them
    /// to CESU-8 and then fails to decode them as UTF-8.
    pub fn to_utf16(&self) -> Result<Vec<u16>> {
        let mut len = mem::MaybeUninit::uninit();
        let ptr = unsafe {
            qjs::JS_ToCStringLenUTF16(self.0.ctx.as_ptr(), len.as_mut_ptr(), self.0.as_js_value())
        };
        if ptr.is_null() {
            // Might not ever happen but I am not 100% sure
            // so just incase check it.
            return Err(Error::Unknown);
        }
        let len = unsafe { len.assume_init() };
        // SAFETY: `ptr` points to `len` units owned by this call. The copy
        // below outlives the free.
        let units: &[u16] = unsafe { slice::from_raw_parts(ptr, len as _) };
        let result = units.to_vec();
        unsafe { qjs::JS_FreeCStringUTF16(self.0.ctx.as_ptr(), ptr) };
        Ok(result)
    }
}

/// Rust representation of a JavaScript C string.
#[derive(Debug)]
pub struct CString<'js> {
    ptr: NonNull<c_char>,
    len: usize,
    ctx: Ctx<'js>,
}

impl<'js> CString<'js> {
    /// Create a new JavaScript C string from a JavaScript string.
    pub fn from_string(string: String<'js>) -> Result<Self> {
        let mut len = mem::MaybeUninit::uninit();
        // SAFETY: The pointer points to a JSString content which is ref counted
        let ptr = unsafe {
            qjs::JS_ToCStringLen(string.0.ctx.as_ptr(), len.as_mut_ptr(), string.as_raw())
        };
        if ptr.is_null() {
            // Might not ever happen but I am not 100% sure
            // so just incase check it.
            return Err(Error::Unknown);
        }
        let len = unsafe { len.assume_init() };
        Ok(Self {
            ptr: unsafe { NonNull::new_unchecked(ptr as *mut _) },
            len,
            ctx: string.0.ctx.clone(),
        })
    }

    /// Converts a `CString` to a raw pointer.
    pub fn as_ptr(&self) -> *const c_char {
        self.ptr.as_ptr() as *const _
    }

    /// Returns the length of this `CString`, in bytes (not chars or graphemes).
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if this `CString` has a length of zero, and `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Extracts a string slice containing the entire `CString`.
    pub fn as_str(&self) -> &str {
        // SAFETY: The pointer points to a JSString content which is ref counted
        let bytes = unsafe { slice::from_raw_parts(self.ptr.as_ptr() as *const u8, self.len) };
        // SAFETY: The bytes are garanteed to be valid utf8 by QuickJS
        unsafe { str::from_utf8_unchecked(bytes) }
    }
}

impl<'js> Drop for CString<'js> {
    fn drop(&mut self) {
        unsafe { qjs::JS_FreeCString(self.ctx.as_ptr(), self.ptr.as_ptr()) };
    }
}
impl<'js> AsRef<str> for CString<'js> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
#[cfg(feature = "std")]
impl<'js> AsRef<std::path::Path> for CString<'js> {
    fn as_ref(&self) -> &std::path::Path {
        std::path::Path::new(self.as_str())
    }
}
impl<'js> core::ops::Deref for CString<'js> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
#[cfg(feature = "std")]
impl<'js> AsRef<std::ffi::OsStr> for CString<'js> {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.as_str().as_ref()
    }
}
impl<'js> AsRef<[u8]> for CString<'js> {
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_ref()
    }
}
impl<'js> PartialEq for CString<'js> {
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(self.as_str(), other.as_str())
    }
}
impl<'js> Eq for CString<'js> {}
impl<'js> core::hash::Hash for CString<'js> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(self.as_str(), state)
    }
}

#[cfg(test)]
mod test {
    use crate::{prelude::*, *};
    #[test]
    fn from_javascript() {
        test_with(|ctx| {
            let s: String = ctx.eval(" 'foo bar baz' ").unwrap();
            assert_eq!(s.to_string().unwrap(), "foo bar baz");
        });
    }

    #[test]
    fn to_javascript() {
        test_with(|ctx| {
            let string = String::from_str(ctx.clone(), "foo").unwrap();
            let func: Function = ctx.eval("x =>  x + 'bar'").unwrap();
            let text: StdString = (string,).apply(&func).unwrap();
            assert_eq!(text, "foobar".to_string());
        });
    }

    #[test]
    fn from_javascript_c() {
        test_with(|ctx| {
            let s: CString = ctx.eval(" 'foo bar baz' ").unwrap();
            assert_eq!(s.as_str(), "foo bar baz");
        });
    }

    #[test]
    fn to_javascript_c() {
        test_with(|ctx| {
            let string = String::from_str(ctx.clone(), "foo")
                .unwrap()
                .to_cstring()
                .unwrap();
            let func: Function = ctx.eval("x =>  x + 'bar'").unwrap();
            let text: StdString = (string,).apply(&func).unwrap();
            assert_eq!(text, "foobar".to_string());
        });
    }

    #[test]
    fn utf16_round_trip() {
        test_with(|ctx| {
            // 'hi' plus U+1F600 as a surrogate pair.
            let units = [0x68, 0x69, 0xD83D, 0xDE00];
            let string = String::from_utf16(ctx.clone(), &units).unwrap();
            assert_eq!(string.to_utf16().unwrap(), units);
        });
    }

    #[test]
    fn utf16_lone_surrogate() {
        test_with(|ctx| {
            let string = String::from_utf16(ctx.clone(), &[0xD800]).unwrap();
            assert_eq!(string.to_utf16().unwrap(), [0xD800]);
            let from_js: String = ctx.eval("'\\uD800'").unwrap();
            assert_eq!(from_js.to_utf16().unwrap(), [0xD800]);
        });
    }

    #[test]
    fn utf16_empty() {
        test_with(|ctx| {
            let string = String::from_utf16(ctx.clone(), &[]).unwrap();
            assert!(string.to_utf16().unwrap().is_empty());
        });
    }

    #[test]
    fn rope_string() {
        test_with(|ctx| {
            let val: Value = ctx
                .eval(
                    r#"
                    let s = "";
                    for (let i = 0; i < 10000; i++) s += "Line " + i + "\n";
                    s
                "#,
                )
                .unwrap();

            assert_eq!(val.type_of(), Type::String);
            assert!(val.is_string());
            assert!(val.as_string().is_some());

            let s: StdString = val.as_string().unwrap().to_string().unwrap();
            assert!(s.starts_with("Line 0\n"));
            assert!(s.contains("Line 999\n"));
            assert!(s.len() > 8000);
        });
    }
}
