//! `FolderRemote` and `Database`.

use std::sync::{Arc, RwLock};

use napi_derive::napi;

use crate::collection::Collection;
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

/// What one `sync()` did (the same fields as the Python package's report).
#[napi(object)]
pub struct SyncReport {
    pub pushed_ops: u32,
    pub segments_up: u32,
    pub segments_down: u32,
    pub pulled_ops: u32,
    pub changed_docs: u32,
    pub snapshots_down: u32,
    pub snapshots_up: u32,
    pub password_changed: bool,
    pub skipped: Vec<String>,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl From<bothy::SyncReport> for SyncReport {
    fn from(r: bothy::SyncReport) -> Self {
        Self {
            pushed_ops: count(r.pushed_ops),
            segments_up: count(r.segments_up),
            segments_down: count(r.segments_down),
            pulled_ops: count(r.pulled_ops),
            changed_docs: count(r.changed_docs),
            snapshots_down: count(r.snapshots_down),
            snapshots_up: count(r.snapshots_up),
            password_changed: r.password_changed,
            skipped: r.skipped,
        }
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
    let db = bothy::Database::open(config, remote)
        .await
        .map_err(from_engine)?;
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

    /// Pushes local changes and pulls remote ones.
    #[napi]
    pub async fn sync(&self) -> Result<SyncReport> {
        let db = self.slot.get()?;
        Ok(db.sync().await.map_err(from_engine)?.into())
    }

    /// A collection of documents. The name is checked now; the database may be closed later.
    #[napi]
    pub fn collection(&self, name: String) -> Result<Collection> {
        bothy::CollectionName::new(&name).map_err(from_engine)?;
        Ok(Collection {
            slot: self.slot.clone(),
            name,
        })
    }

    /// Releases the local database. Calling it again is fine.
    #[napi]
    pub async fn close(&self) -> Result<()> {
        drop(self.slot.take());
        Ok(())
    }
}
