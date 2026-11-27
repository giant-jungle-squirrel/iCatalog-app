use crate::command::SyncProgress;
use crate::error::{EngineError, Result};
use std::path::{Path, PathBuf};

/// An open catalogue session.
pub struct Context {
    path: PathBuf,
    catalog: icatalog_catalog::Catalog,
    read_only: bool,
    migrations_deferred: bool,
    progress: Option<std::sync::Arc<dyn Fn(SyncProgress) + Send + Sync>>,
}

impl Context {
    /// Open a session backed by an in-memory catalogue, primarily for tests.
    pub fn open_in_memory() -> Result<Self> {
        Ok(Context {
            path: PathBuf::from(":memory:"),
            catalog: icatalog_catalog::Catalog::open_in_memory()?,
            read_only: false,
            migrations_deferred: false,
            progress: None,
        })
    }

    /// Prepare a destination session without opening, creating or migrating
    /// its file. Used by commands that install a verified new catalogue.
    pub fn for_destination(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Context {
            path: path.as_ref().to_path_buf(),
            catalog: icatalog_catalog::Catalog::open_in_memory()?,
            read_only: false,
            migrations_deferred: false,
            progress: None,
        })
    }

    /// Open (creating if absent) the catalogue at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (catalog, migrations_deferred) = open_catalogue(&path, true)?;
        Ok(Context {
            path,
            catalog,
            read_only: false,
            migrations_deferred,
            progress: None,
        })
    }

    /// Open an existing on-disk catalogue without creating a missing file.
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            return Err(crate::error::EngineError::BadParam(
                "catalogue must be an existing file; check its path and mounts, or run init explicitly"
                    .into(),
            ));
        }
        // Removing SQLite's create flag also covers removal between the check
        // above and opening the connection.
        let (catalog, migrations_deferred) = open_catalogue(&path, false)?;
        Ok(Context {
            path,
            catalog,
            read_only: false,
            migrations_deferred,
            progress: None,
        })
    }

    /// Open a catalogue using SQLite read-only flags, without migrations.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let catalog = icatalog_catalog::Catalog::open_read_only(&path)?;
        Ok(Context {
            path,
            catalog,
            read_only: true,
            migrations_deferred: false,
            progress: None,
        })
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// The main file is held read-only until an interrupted sync snapshot has
    /// been verified, or unreadable recovery state has been repaired. Unlike an
    /// explicitly opened backup, this session still permits recovery commands.
    pub fn migrations_deferred(&self) -> bool {
        self.migrations_deferred
    }

    /// Restore the normal catalogue connection only after the sync journal no
    /// longer needs the pre-migration bytes for digest/generation verification.
    pub fn resume_after_sync_recovery(&mut self) -> Result<()> {
        let (catalog, deferred) = open_catalogue(&self.path, false)?;
        self.catalog = catalog;
        self.migrations_deferred = deferred;
        if deferred {
            return Err(EngineError::Unsupported(
                "sync recovery is pending; inspect sync-resolve list and recover before editing"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Install a task-local progress callback. No callback is retained by default.
    pub fn set_progress_sink(
        &mut self,
        sink: Option<std::sync::Arc<dyn Fn(SyncProgress) + Send + Sync>>,
    ) {
        self.progress = sink;
    }

    pub fn report_progress(&self, progress: SyncProgress) {
        if let Some(sink) = &self.progress {
            sink(progress);
        }
    }

    /// Durable per-catalogue sync-required latch, kept outside the replaceable catalogue.
    pub fn sync_required(&self) -> Result<bool> {
        let Some(path) = self.sync_state_path() else {
            return Ok(false);
        };
        if !path.is_file() {
            return Ok(self.migrations_deferred);
        }
        let row = self
            .catalog
            .catalogue()?
            .ok_or_else(|| EngineError::Unsupported("catalogue is not initialised".into()))?;
        let state = icatalog_catalog::sync_state::SyncStateDb::open(&self.path, &row.id, false)?;
        let recovery_pending = state.snapshot_recovery_pending()?;
        Ok(state.required()? || recovery_pending || self.migrations_deferred)
    }

    /// Set or clear the durable sync-required latch. It is independent of a backup/snapshot replacement.
    pub fn set_sync_required(&self, required: bool) -> Result<()> {
        let row = self
            .catalog
            .catalogue()?
            .ok_or_else(|| EngineError::Unsupported("catalogue is not initialised".into()))?;
        icatalog_catalog::sync_state::SyncStateDb::open(&self.path, &row.id, true)?
            .set_required(required)?;
        Ok(())
    }

    /// Companion SQLite state path shared by sync and recovery workflows.
    pub fn sync_state_path(&self) -> Option<PathBuf> {
        if self.path == Path::new(":memory:") {
            return None;
        }
        icatalog_catalog::sync_state::companion_path(&self.path).ok()
    }

    /// Catalogue file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Shared access to the catalogue.
    pub fn catalog(&self) -> &icatalog_catalog::Catalog {
        &self.catalog
    }

    /// Mutable access to the catalogue.
    pub fn catalog_mut(&mut self) -> &mut icatalog_catalog::Catalog {
        &mut self.catalog
    }

    /// Require a regular pathname before replacing this catalogue file.
    /// Replacing a linked pathname would detach the new replica from its aliases.
    pub fn ensure_replaceable_path(&self) -> Result<()> {
        let metadata = std::fs::symlink_metadata(&self.path)?;
        let kind = metadata.file_type();
        if kind.is_symlink() {
            return Err(EngineError::Unsupported(
                "sync cannot replace a catalogue symlink; open its real file path".into(),
            ));
        }
        if !kind.is_file() {
            return Err(EngineError::Unsupported(
                "sync requires a regular catalogue file".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() > 1 {
                return Err(EngineError::Unsupported(
                    "sync cannot replace a catalogue with hard links; use a separate copy".into(),
                ));
            }
        }
        #[cfg(windows)]
        {
            let file = std::fs::File::open(&self.path)?;
            let info = winapi_util::file::information(&file)?;
            if info.number_of_links() > 1 {
                return Err(EngineError::Unsupported(
                    "sync cannot replace a catalogue with hard links; use a separate copy".into(),
                ));
            }
        }
        Ok(())
    }

    /// Replace the on-disk catalogue with `temp` (a verified catalogue file on
    /// the same filesystem), closing the connection so the rename is atomic,
    /// then reopen it. On failure the existing file is left in place and the
    /// connection is reopened. Used by the sync client's download-and-swap.
    pub fn replace_catalogue_file(&mut self, temp: &Path) -> Result<()> {
        self.ensure_replaceable_path()?;
        let placeholder = icatalog_catalog::Catalog::open_in_memory()?;
        let old = std::mem::replace(&mut self.catalog, placeholder);
        drop(old);
        if let Err(e) = std::fs::rename(temp, &self.path) {
            let (catalog, deferred) = open_catalogue(&self.path, false)?;
            self.catalog = catalog;
            self.migrations_deferred = deferred;
            return Err(e.into());
        }
        let (catalog, deferred) = open_catalogue(&self.path, false)?;
        self.catalog = catalog;
        self.migrations_deferred = deferred;
        Ok(())
    }
}

/// Opening a recovery companion is read-only and catalogue-ID bound. If its
/// state cannot be inspected, keep the main file untouched and let sync-check /
/// sync-resolve report the original error. This also preserves existing desktop
/// behavior: a damaged companion permits browsing but never enables edits.
fn open_catalogue(path: &Path, create: bool) -> Result<(icatalog_catalog::Catalog, bool)> {
    let companion = icatalog_catalog::sync_state::companion_path(path)?;
    let companion_exists = match std::fs::symlink_metadata(&companion) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if companion_exists {
        let catalog = icatalog_catalog::Catalog::open_read_only(path)?;
        let row = catalog
            .catalogue()?
            .ok_or_else(|| EngineError::Unsupported("catalogue is not initialised".into()))?;
        let pending = icatalog_catalog::sync_state::SyncStateDb::open(path, &row.id, false)
            .and_then(|state| state.snapshot_recovery_pending());
        if !matches!(pending, Ok(false)) {
            return Ok((catalog, true));
        }
    }
    let catalog = if create {
        icatalog_catalog::Catalog::open(path)?
    } else {
        icatalog_catalog::Catalog::open_existing(path)?
    };
    Ok((catalog, false))
}
