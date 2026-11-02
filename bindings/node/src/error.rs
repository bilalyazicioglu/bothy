//! Maps engine errors onto napi errors.
//!
//! napi's generated async code only accepts its default error type, which cannot carry a
//! custom `code`. So the error kind travels as a message prefix, `bothy:<Kind>:<message>`,
//! and the JavaScript facade (`index.js`) turns it into `BothyError.kind`.

pub type Result<T> = napi::Result<T>;

fn tagged(kind: &str, message: &str) -> napi::Error {
    napi::Error::from_reason(format!("bothy:{kind}:{message}"))
}

/// An engine error. The engine's messages never contain tokens, passwords or keys.
pub fn from_engine(e: bothy::Error) -> napi::Error {
    tagged(&format!("{:?}", e.kind()), &e.to_string())
}

pub fn invalid_input(message: &str) -> napi::Error {
    tagged("InvalidInput", message)
}
