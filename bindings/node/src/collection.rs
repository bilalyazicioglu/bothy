//! `Collection`: `put` and `get`.

use napi_derive::napi;

use crate::database::DbSlot;
use crate::error::{Result, from_engine};
use crate::value::{JsDoc, JsValue, Str};

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
    pub async fn put(&self, id: Str, doc: JsDoc) -> Result<bool> {
        let open = self.slot.acquire().await?;
        let collection = open.db().collection(&self.name).map_err(from_engine)?;
        collection.put(&id.0, doc.0).await.map_err(from_engine)
    }

    /// The document, or `null`.
    #[napi]
    pub async fn get(&self, id: Str) -> Result<Option<JsValue>> {
        let open = self.slot.acquire().await?;
        let collection = open.db().collection(&self.name).map_err(from_engine)?;
        Ok(collection
            .get(&id.0)
            .await
            .map_err(from_engine)?
            .map(JsValue))
    }
}
