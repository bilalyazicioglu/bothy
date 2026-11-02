//! JavaScript values to and from `bothy::Value`, without going through JSON.
//!
//! Writing: a safe-integer `number` is an `Int`, any other finite `number` a `Float`, a
//! `bigint` within i64 an `Int`, a `Uint8Array` `Bytes`; strings, booleans, `null`, arrays and
//! plain objects map to their own types. Everything else is `InvalidInput`.
//!
//! Reading: an `Int` in the safe range is a `number`, any other `Int` a `bigint`, a `Float` a
//! `number`, `Bytes` a `Uint8Array`.
//!
//! These are the raw N-API calls the high-level types cannot express: strings are read as
//! UTF-16 so a lone surrogate is an error and not silently U+FFFD, objects are built with
//! `define` semantics so a `"__proto__"` key stays an own property, and byte arrays are copied
//! (no external buffers, which Electron forbids).

use std::collections::BTreeMap;
use std::ffi::c_char;
use std::ptr;

use bothy::Value;
use napi::bindgen_prelude::{FromNapiValue, ToNapiValue, TypeName, ValidateNapiValue};
use napi::{ValueType, sys};

use crate::error::{Result, invalid_input};

/// The engine allows arrays and maps nested up to this depth (`Value::validate`).
const MAX_DEPTH: usize = 32;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// A whole document argument: a plain object.
pub struct JsDoc(pub Value);

/// A string argument (an id, a name, a path). Read as UTF-16, so a lone surrogate is an error
/// and two different ids can never collapse into one through U+FFFD.
pub struct Str(pub String);

/// Any value, for results.
pub struct JsValue(pub Value);

fn check(status: sys::napi_status, what: &str) -> Result<()> {
    if status == sys::Status::napi_ok {
        Ok(())
    } else {
        Err(napi::Error::from_reason(format!(
            "bothy:Internal:{what} failed ({status})"
        )))
    }
}

/// Per-conversion state: the environment and `Object.prototype`, for the plain-object check.
struct Reader {
    env: sys::napi_env,
    object_proto: sys::napi_value,
    reflect: sys::napi_value,
    get_prototype_of: sys::napi_value,
}

impl Reader {
    unsafe fn new(env: sys::napi_env) -> Result<Self> {
        let mut global = ptr::null_mut();
        check(
            unsafe { sys::napi_get_global(env, &mut global) },
            "napi_get_global",
        )?;
        let object = unsafe { get_named(env, global, "Object")? };
        let object_proto = unsafe { get_named(env, object, "prototype")? };
        // `Reflect.getPrototypeOf` looks through a Proxy; `napi_get_prototype` reports null.
        let reflect = unsafe { get_named(env, global, "Reflect")? };
        let get_prototype_of = unsafe { get_named(env, reflect, "getPrototypeOf")? };
        Ok(Self {
            env,
            object_proto,
            reflect,
            get_prototype_of,
        })
    }

    unsafe fn value(&self, v: sys::napi_value, depth: usize) -> Result<Value> {
        let env = self.env;
        let mut ty = 0;
        check(unsafe { sys::napi_typeof(env, v, &mut ty) }, "napi_typeof")?;
        match ty {
            t if t == ValueType::Null as sys::napi_valuetype => Ok(Value::Null),
            t if t == ValueType::Boolean as sys::napi_valuetype => {
                let mut b = false;
                check(
                    unsafe { sys::napi_get_value_bool(env, v, &mut b) },
                    "napi_get_value_bool",
                )?;
                Ok(Value::Bool(b))
            }
            t if t == ValueType::Number as sys::napi_valuetype => {
                let mut f = 0.0;
                check(
                    unsafe { sys::napi_get_value_double(env, v, &mut f) },
                    "napi_get_value_double",
                )?;
                number(f)
            }
            t if t == ValueType::BigInt as sys::napi_valuetype => {
                let (mut i, mut lossless) = (0i64, false);
                check(
                    unsafe { sys::napi_get_value_bigint_int64(env, v, &mut i, &mut lossless) },
                    "napi_get_value_bigint_int64",
                )?;
                if lossless {
                    Ok(Value::Int(i))
                } else {
                    Err(invalid_input(
                        "bigint does not fit in a signed 64-bit integer",
                    ))
                }
            }
            t if t == ValueType::String as sys::napi_valuetype => {
                Ok(Value::Text(unsafe { string(env, v)? }))
            }
            t if t == ValueType::Object as sys::napi_valuetype => unsafe { self.object(v, depth) },
            t if t == ValueType::Undefined as sys::napi_valuetype => Err(invalid_input(
                "undefined cannot be stored; leave the field out or use null",
            )),
            _ => Err(invalid_input(
                "a symbol, function or external cannot be stored",
            )),
        }
    }

    unsafe fn object(&self, v: sys::napi_value, depth: usize) -> Result<Value> {
        let env = self.env;
        if depth > MAX_DEPTH {
            return Err(invalid_input("value nesting is deeper than 32"));
        }
        let mut flag = false;
        check(
            unsafe { sys::napi_is_array(env, v, &mut flag) },
            "napi_is_array",
        )?;
        if flag {
            return unsafe { self.array(v, depth) };
        }
        check(
            unsafe { sys::napi_is_typedarray(env, v, &mut flag) },
            "napi_is_typedarray",
        )?;
        if flag {
            return unsafe { bytes(env, v) };
        }
        check(
            unsafe { sys::napi_is_arraybuffer(env, v, &mut flag) },
            "napi_is_arraybuffer",
        )?;
        if flag {
            return Err(invalid_input(
                "an ArrayBuffer cannot be stored; use a Uint8Array",
            ));
        }
        unsafe { self.plain_object(v, depth) }
    }

    unsafe fn array(&self, v: sys::napi_value, depth: usize) -> Result<Value> {
        let env = self.env;
        let mut len = 0u32;
        check(
            unsafe { sys::napi_get_array_length(env, v, &mut len) },
            "napi_get_array_length",
        )?;
        let mut items = Vec::new();
        for i in 0..len {
            let mut has = false;
            check(
                unsafe { sys::napi_has_element(env, v, i, &mut has) },
                "napi_has_element",
            )?;
            if !has {
                return Err(invalid_input("an array with holes cannot be stored"));
            }
            let mut item = ptr::null_mut();
            check(
                unsafe { sys::napi_get_element(env, v, i, &mut item) },
                "napi_get_element",
            )?;
            items.push(unsafe { self.value(item, depth + 1)? });
        }
        Ok(Value::Array(items))
    }

    unsafe fn prototype_of(&self, v: sys::napi_value) -> Result<sys::napi_value> {
        let mut proto = ptr::null_mut();
        let argv = [v];
        check(
            unsafe {
                sys::napi_call_function(
                    self.env,
                    self.reflect,
                    self.get_prototype_of,
                    1,
                    argv.as_ptr(),
                    &mut proto,
                )
            },
            "Reflect.getPrototypeOf",
        )?;
        Ok(proto)
    }

    unsafe fn is_null(&self, v: sys::napi_value) -> Result<bool> {
        let mut ty = 0;
        check(
            unsafe { sys::napi_typeof(self.env, v, &mut ty) },
            "napi_typeof",
        )?;
        Ok(ty == ValueType::Null as sys::napi_valuetype)
    }

    unsafe fn plain_object(&self, v: sys::napi_value, depth: usize) -> Result<Value> {
        let env = self.env;
        let proto = unsafe { self.prototype_of(v)? };
        let mut plain = unsafe { self.is_null(proto)? };
        if !plain {
            check(
                unsafe { sys::napi_strict_equals(env, proto, self.object_proto, &mut plain) },
                "napi_strict_equals",
            )?;
        }
        if !plain {
            // `Object.prototype` of another realm (`node:vm`, Jest) is not ours, but it is the
            // one prototype in the chain that has no prototype itself. A class, `Date` or `Map`
            // prototype always has one.
            let parent = unsafe { self.prototype_of(proto)? };
            plain = unsafe { self.is_null(parent)? };
        }
        if !plain {
            return Err(invalid_input(
                "only plain objects can be stored (not a Date, Map, Set or class instance)",
            ));
        }
        let mut names = ptr::null_mut();
        check(
            unsafe {
                sys::napi_get_all_property_names(
                    env,
                    v,
                    sys::KeyCollectionMode::own_only,
                    sys::KeyFilter::enumerable | sys::KeyFilter::skip_symbols,
                    sys::KeyConversion::numbers_to_strings,
                    &mut names,
                )
            },
            "napi_get_all_property_names",
        )?;
        let mut len = 0u32;
        check(
            unsafe { sys::napi_get_array_length(env, names, &mut len) },
            "napi_get_array_length",
        )?;
        let mut map = BTreeMap::new();
        for i in 0..len {
            let mut key = ptr::null_mut();
            check(
                unsafe { sys::napi_get_element(env, names, i, &mut key) },
                "napi_get_element",
            )?;
            let mut item = ptr::null_mut();
            check(
                unsafe { sys::napi_get_property(env, v, key, &mut item) },
                "napi_get_property",
            )?;
            let name = unsafe { string(env, key)? };
            map.insert(name, unsafe { self.value(item, depth + 1)? });
        }
        Ok(Value::Map(map))
    }
}

fn number(f: f64) -> Result<Value> {
    if !f.is_finite() {
        return Err(invalid_input("NaN and Infinity cannot be stored"));
    }
    if f.fract() != 0.0 {
        return Ok(Value::Float(f));
    }
    if f.abs() <= MAX_SAFE_INTEGER {
        // Includes -0, which is the integer 0.
        return Ok(Value::Int(f as i64));
    }
    Err(invalid_input(
        "a number that is an integer above 2^53 cannot be stored exactly; use a bigint",
    ))
}

/// A string, read as UTF-16 so that a lone surrogate is an error.
unsafe fn string(env: sys::napi_env, v: sys::napi_value) -> Result<String> {
    let mut len = 0usize;
    check(
        unsafe { sys::napi_get_value_string_utf16(env, v, ptr::null_mut(), 0, &mut len) },
        "napi_get_value_string_utf16",
    )?;
    let mut buf = vec![0u16; len + 1];
    let mut copied = 0usize;
    check(
        unsafe {
            sys::napi_get_value_string_utf16(env, v, buf.as_mut_ptr(), buf.len(), &mut copied)
        },
        "napi_get_value_string_utf16",
    )?;
    String::from_utf16(&buf[..copied])
        .map_err(|_| invalid_input("a string with a lone surrogate cannot be stored"))
}

unsafe fn bytes(env: sys::napi_env, v: sys::napi_value) -> Result<Value> {
    let mut kind = 0;
    let mut len = 0usize;
    let mut data = ptr::null_mut();
    let mut buffer = ptr::null_mut();
    let mut offset = 0usize;
    check(
        unsafe {
            sys::napi_get_typedarray_info(
                env,
                v,
                &mut kind,
                &mut len,
                &mut data,
                &mut buffer,
                &mut offset,
            )
        },
        "napi_get_typedarray_info",
    )?;
    if kind != sys::TypedarrayType::uint8_array {
        return Err(invalid_input("only a Uint8Array can be stored as bytes"));
    }
    if len == 0 {
        return Ok(Value::Bytes(Vec::new()));
    }
    Ok(Value::Bytes(
        unsafe { std::slice::from_raw_parts(data as *const u8, len) }.to_vec(),
    ))
}

unsafe fn get_named(
    env: sys::napi_env,
    obj: sys::napi_value,
    name: &str,
) -> Result<sys::napi_value> {
    let mut key = ptr::null_mut();
    check(
        unsafe {
            sys::napi_create_string_utf8(
                env,
                name.as_ptr() as *const c_char,
                name.len() as _,
                &mut key,
            )
        },
        "napi_create_string_utf8",
    )?;
    let mut out = ptr::null_mut();
    check(
        unsafe { sys::napi_get_property(env, obj, key, &mut out) },
        "napi_get_property",
    )?;
    Ok(out)
}

/// Converts a JS value; the result passes the engine's own validity check.
unsafe fn read(env: sys::napi_env, v: sys::napi_value) -> Result<Value> {
    let value = unsafe { Reader::new(env)?.value(v, 0)? };
    value.validate().map_err(crate::error::from_engine)?;
    Ok(value)
}

unsafe fn write(env: sys::napi_env, value: &Value) -> Result<sys::napi_value> {
    let mut out = ptr::null_mut();
    match value {
        Value::Null => check(
            unsafe { sys::napi_get_null(env, &mut out) },
            "napi_get_null",
        )?,
        Value::Bool(b) => check(
            unsafe { sys::napi_get_boolean(env, *b, &mut out) },
            "napi_get_boolean",
        )?,
        Value::Int(i) if i.unsigned_abs() <= MAX_SAFE_INTEGER as u64 => check(
            unsafe { sys::napi_create_double(env, *i as f64, &mut out) },
            "napi_create_double",
        )?,
        Value::Int(i) => check(
            unsafe { sys::napi_create_bigint_int64(env, *i, &mut out) },
            "napi_create_bigint_int64",
        )?,
        Value::Float(f) => check(
            unsafe { sys::napi_create_double(env, *f, &mut out) },
            "napi_create_double",
        )?,
        Value::Text(s) => out = unsafe { create_string(env, s)? },
        Value::Bytes(b) => out = unsafe { create_bytes(env, b)? },
        Value::Array(items) => {
            check(
                unsafe { sys::napi_create_array_with_length(env, items.len(), &mut out) },
                "napi_create_array_with_length",
            )?;
            for (i, item) in items.iter().enumerate() {
                let element = unsafe { write(env, item)? };
                check(
                    unsafe { sys::napi_set_element(env, out, i as u32, element) },
                    "napi_set_element",
                )?;
            }
        }
        Value::Map(map) => {
            check(
                unsafe { sys::napi_create_object(env, &mut out) },
                "napi_create_object",
            )?;
            for (key, item) in map {
                let name = unsafe { create_string(env, key)? };
                let element = unsafe { write(env, item)? };
                // Define, not set: a "__proto__" key must become an own data property.
                let descriptor = sys::napi_property_descriptor {
                    utf8name: ptr::null(),
                    name,
                    method: None,
                    getter: None,
                    setter: None,
                    value: element,
                    attributes: sys::PropertyAttributes::writable
                        | sys::PropertyAttributes::enumerable
                        | sys::PropertyAttributes::configurable,
                    data: ptr::null_mut(),
                };
                check(
                    unsafe { sys::napi_define_properties(env, out, 1, &descriptor) },
                    "napi_define_properties",
                )?;
            }
        }
    }
    Ok(out)
}

unsafe fn create_string(env: sys::napi_env, s: &str) -> Result<sys::napi_value> {
    let mut out = ptr::null_mut();
    check(
        unsafe {
            sys::napi_create_string_utf8(env, s.as_ptr() as *const c_char, s.len() as _, &mut out)
        },
        "napi_create_string_utf8",
    )?;
    Ok(out)
}

/// A `Uint8Array` over a copy of `b`.
unsafe fn create_bytes(env: sys::napi_env, b: &[u8]) -> Result<sys::napi_value> {
    let mut data = ptr::null_mut();
    let mut buffer = ptr::null_mut();
    check(
        unsafe { sys::napi_create_arraybuffer(env, b.len(), &mut data, &mut buffer) },
        "napi_create_arraybuffer",
    )?;
    if !b.is_empty() {
        unsafe { ptr::copy_nonoverlapping(b.as_ptr(), data as *mut u8, b.len()) };
    }
    let mut out = ptr::null_mut();
    check(
        unsafe {
            sys::napi_create_typedarray(
                env,
                sys::TypedarrayType::uint8_array,
                b.len(),
                buffer,
                0,
                &mut out,
            )
        },
        "napi_create_typedarray",
    )?;
    Ok(out)
}

impl TypeName for JsDoc {
    fn type_name() -> &'static str {
        "Record<string, unknown>"
    }
    fn value_type() -> ValueType {
        ValueType::Object
    }
}

impl ValidateNapiValue for JsDoc {}

impl FromNapiValue for JsDoc {
    unsafe fn from_napi_value(env: sys::napi_env, v: sys::napi_value) -> napi::Result<Self> {
        match unsafe { read(env, v)? } {
            doc @ Value::Map(_) => Ok(JsDoc(doc)),
            _ => Err(invalid_input("a document must be a plain object")),
        }
    }
}

impl TypeName for JsValue {
    fn type_name() -> &'static str {
        "Record<string, unknown>"
    }
    fn value_type() -> ValueType {
        ValueType::Object
    }
}

impl ToNapiValue for JsValue {
    unsafe fn to_napi_value(env: sys::napi_env, val: Self) -> napi::Result<sys::napi_value> {
        unsafe { write(env, &val.0) }
    }
}

impl TypeName for Str {
    fn type_name() -> &'static str {
        "string"
    }
    fn value_type() -> ValueType {
        ValueType::String
    }
}

impl ValidateNapiValue for Str {}

impl FromNapiValue for Str {
    unsafe fn from_napi_value(env: sys::napi_env, v: sys::napi_value) -> napi::Result<Self> {
        let mut ty = 0;
        check(unsafe { sys::napi_typeof(env, v, &mut ty) }, "napi_typeof")?;
        if ty != ValueType::String as sys::napi_valuetype {
            return Err(invalid_input("expected a string"));
        }
        Ok(Str(unsafe { string(env, v)? }))
    }
}
