//! `FolderRemote` and `Database`.

use std::sync::{Arc, RwLock};

use napi_derive::napi;

use crate::error::{Result, from_engine, invalid_input};

/// A folder shared by the devices (a local or synced directory).
#[napi]
pub struct FolderRemote {
    path: String,
}

#[napi]
impl FolderRemote {
    #[napi(constructor)]
    pub fn new(path: String) -> Result<Self> {
        // The engine would take "" as the working directory and sync into it silently.
        if path.is_empty() {
            return Err(invalid_input("remote folder path is empty"));
        }
        // Validates the path and creates the folder, like the engine does on open.
        bothy::remote::FolderRemote::new(&path).map_err(from_engine)?;
        Ok(Self { path })
    }

    #[napi(getter)]
    pub fn path(&self) -> String {
        self.path.clone()
    }
}

/// The engine handle shared by a `Database` and its collections. `close()` empties it, so
/// nothing keeps the local database open afterwards.
#[derive(Clone, Default)]
pub(crate) struct DbSlot(Arc<RwLock<Option<bothy::Database>>>);

impl DbSlot {
    /// A handle to the engine for one call; fails once the database is closed.
    pub(crate) fn get(&self) -> Result<bothy::Database> {
        self.0
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| invalid_input("database is closed"))
    }

    fn take(&self) -> Option<bothy::Database> {
        self.0.write().unwrap_or_else(|e| e.into_inner()).take()
    }
}

#[napi]
pub struct Database {
    slot: DbSlot,
}

/// Opens (or creates) the local database. The facade's `Database.open` calls this.
#[napi]
pub async fn open_database(
    path: String,
    app_id: String,
    remote_path: String,
    device_name: Option<String>,
) -> Result<Database> {
    let remote = Arc::new(bothy::remote::FolderRemote::new(&remote_path).map_err(from_engine)?);
    let mut config = bothy::Config::new(path, &app_id);
    if let Some(name) = &device_name {
        config = config.device_name(name);
    }
    let db = bothy::Database::open(config, remote).await.map_err(from_engine)?;
    Ok(Database {
        slot: DbSlot(Arc::new(RwLock::new(Some(db)))),
    })
}

#[napi]
impl Database {
    /// Creates the database on the remote or joins the one that is there; returns its id (hex).
    #[napi]
    pub async fn init_or_join(&self) -> Result<String> {
        let db = self.slot.get()?;
        Ok(db.init_or_join().await.map_err(from_engine)?.to_hex())
    }

    /// Releases the local database. Calling it again is fine.
    #[napi]
    pub async fn close(&self) -> Result<()> {
        drop(self.slot.take());
        Ok(())
    }
}
