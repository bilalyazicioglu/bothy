//! Native module of `@bothy-sync/node`. The JavaScript facade in `index.js` wraps it.

mod collection;
mod database;
mod error;
mod value;

use napi_derive::napi;

/// The version of the Bothy engine this module was built with.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
