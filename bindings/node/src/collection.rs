//! `Collection`: `put` and `get`.

use napi_derive::napi;

use crate::database::DbSlot;
use crate::error::{Result, from_engine};
use crate::value::{JsDoc, JsValue};

/// A named group of documents. It holds the database's shared slot, not an engine handle, so
/// closing the database really releases it.
#[napi]
pub struct Collection {
    pub(crate) slot: DbSlot,
    pub(crate) name: String,
}

#[napi]
impl Collection {
    /// Writes a whole document. Returns whether anything changed.
    #[napi]
    pub async fn put(&self, id: String, doc: JsDoc) -> Result<bool> {
        let collection = self
            .slot
            .get()?
            .collection(&self.name)
            .map_err(from_engine)?;
        collection.put(&id, doc.0).await.map_err(from_engine)
    }

    /// The document, or `null`.
    #[napi]
    pub async fn get(&self, id: String) -> Result<Option<JsValue>> {
        let collection = self
            .slot
            .get()?
            .collection(&self.name)
            .map_err(from_engine)?;
        Ok(collection.get(&id).await.map_err(from_engine)?.map(JsValue))
    }
}
