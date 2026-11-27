//! Pending v8 replacements retain their exact bytes until explicit recovery.
#![allow(clippy::unwrap_used, clippy::panic)]
use icatalog_catalog::{sync_state::SyncStateDb, Catalog};
use icatalog_engine::{Context, Outcome, Params, SyncOutcome};
use std::path::{Path, PathBuf};

struct Fixture {
    path: PathBuf,
    stage: PathBuf,
    backup: PathBuf,
    companion: PathBuf,
    digest: String,
}

fn encoded_path(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = vec![b'u'];
        bytes.extend_from_slice(path.as_os_str().as_bytes());
        bytes
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut bytes = vec![b'w'];
        for word in path.as_os_str().encode_wide() {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }
}

fn fixture(root: &Path, consumed: bool) -> Fixture {
    let path = root.join("replica.icatalog");
    let stage = root.join(".download.icatalog-sync-v8-recovery");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_utc TEXT NOT NULL);").unwrap();
    for (index, sql) in [
        include_str!("../../icatalog-catalog/src/v1.sql"),
        include_str!("../../icatalog-catalog/src/v2.sql"),
        include_str!("../../icatalog-catalog/src/v3.sql"),
        include_str!("../../icatalog-catalog/src/v4.sql"),
        include_str!("../../icatalog-catalog/src/v5.sql"),
        include_str!("../../icatalog-catalog/src/v6.sql"),
        include_str!("../../icatalog-catalog/src/v7.sql"),
        include_str!("../../icatalog-catalog/src/v8.sql"),
    ]
    .into_iter()
    .enumerate()
    {
        conn.execute_batch(sql).unwrap();
        conn.execute("INSERT INTO schema_migrations VALUES(?1,'t')", [index + 1])
            .unwrap();
    }
    conn.execute_batch("INSERT INTO catalogue VALUES('upgrade-recovery',0,'t',8);INSERT INTO asset(id,uid,name,path,format) VALUES(1,'one','painting.reb','/synthetic/painting.reb','REB');INSERT INTO collection(id,name) VALUES(1,'Paintings');INSERT INTO smart_collection(collection_id,format) VALUES(1,'REB');").unwrap();
    let catalog = Catalog::from_existing_connection(conn).unwrap();
    let baseline = icatalog_sync::delta::encode(
        "upgrade-recovery",
        &icatalog_sync::delta::current(&catalog).unwrap(),
    )
    .unwrap();
    catalog.set_replica_base(7, &baseline).unwrap();
    catalog.set_generation(7).unwrap();
    drop(catalog);
    std::fs::copy(&path, &stage).unwrap();
    let staged =
        Catalog::from_existing_connection(rusqlite::Connection::open(&stage).unwrap()).unwrap();
    staged.set_replica_base(9, &baseline).unwrap();
    staged.set_generation(9).unwrap();
    drop(staged);
    let backup = icatalog_sync::util::backup_local(&path).unwrap().unwrap();
    let digest = icatalog_sync::util::sha256_file(&stage).unwrap();
    let state = SyncStateDb::open(&path, "upgrade-recovery", true).unwrap();
    // A false or missing latch must not override the durable snapshot phase.
    state.set_required(false).unwrap();
    state.connection().execute_batch("CREATE TABLE sync_conflict(key TEXT PRIMARY KEY,local_record BLOB NOT NULL,server_record BLOB NOT NULL);CREATE TABLE sync_choice(key TEXT NOT NULL,field TEXT NOT NULL,choice TEXT NOT NULL,PRIMARY KEY(key,field));CREATE TABLE sync_resolution(key TEXT PRIMARY KEY,merged_record BLOB NOT NULL);CREATE TABLE sync_session(singleton INTEGER PRIMARY KEY,session_id TEXT NOT NULL);CREATE TABLE sync_recovery(singleton INTEGER PRIMARY KEY,phase TEXT NOT NULL,backup_path BLOB,staged_path TEXT,local_generation INTEGER,server_generation INTEGER,expected_digest TEXT);").unwrap();
    state
        .connection()
        .execute(
            "INSERT INTO sync_recovery VALUES(1,'snapshot_ready',?1,?2,7,9,?3)",
            rusqlite::params![
                encoded_path(&backup),
                stage.file_name().unwrap().to_str().unwrap(),
                digest
            ],
        )
        .unwrap();
    let companion = state.path().to_owned();
    drop(state);
    if consumed {
        std::fs::rename(&stage, &path).unwrap();
    }
    Fixture {
        path,
        stage,
        backup,
        companion,
        digest,
    }
}

fn recover(ctx: &mut Context) {
    let mut params = Params::new();
    params.set("action", "recover");
    let result = icatalog_sync::dispatch_guarded(ctx, "sync-resolve", &params).unwrap();
    assert!(
        matches!(result, Outcome::Sync(SyncOutcome::Resolve(ref result)) if result.recovered && !result.recovery_pending)
    );
    assert_eq!(ctx.catalog().schema_version().unwrap(), 9);
    assert_eq!(ctx.catalog().generation().unwrap(), Some(10));
    assert_eq!(ctx.catalog().replica_base().unwrap().unwrap().0, 9);
    assert!(!ctx.migrations_deferred());
    assert!(!ctx.sync_required().unwrap());
    let mut edit = Params::new();
    edit.set("id", "1").set("rating", "5");
    icatalog_engine::dispatch(ctx, "edit", &edit).unwrap();
}

#[test]
fn both_context_openers_defer_v8_migration_until_staged_or_consumed_recovery_finishes() {
    for consumed in [false, true] {
        for create in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let f = fixture(dir.path(), consumed);
            let before = std::fs::read(&f.path).unwrap();
            let backup_before = std::fs::read(&f.backup).unwrap();
            let readonly = Context::open_read_only(&f.path).unwrap();
            assert!(readonly.sync_required().unwrap());
            drop(readonly);
            let mut ctx = if create {
                Context::open(&f.path)
            } else {
                Context::open_existing(&f.path)
            }
            .unwrap();
            assert_eq!(ctx.catalog().schema_version().unwrap(), 8);
            assert!(ctx.migrations_deferred());
            assert!(!ctx.is_read_only()); // Recovery remains available in desktop controls.
            assert!(ctx.sync_required().unwrap());
            assert_eq!(ctx.catalog().collection_assets(1).unwrap(), vec![1]);
            icatalog_engine::dispatch(&mut ctx, "list", &Params::new()).unwrap();
            let mut edit = Params::new();
            edit.set("id", "1").set("rating", "4");
            assert!(icatalog_engine::dispatch(&mut ctx, "edit", &edit)
                .unwrap_err()
                .to_string()
                .contains("sync recovery is pending"));
            assert!(ctx.catalog().set_generation(100).is_err());
            let mut list = Params::new();
            list.set("action", "list");
            let outcome = icatalog_sync::dispatch_guarded(&mut ctx, "sync-resolve", &list).unwrap();
            assert!(
                matches!(outcome, Outcome::Sync(SyncOutcome::Resolve(ref result)) if result.recovery_phase.as_deref() == Some("snapshot_ready") && !result.recovered)
            );
            assert!(
                std::fs::read(&f.path).unwrap() == before,
                "main catalogue bytes changed before recovery"
            );
            recover(&mut ctx);
            assert!(!f.stage.exists());
            assert!(
                std::fs::read(&f.backup).unwrap() == backup_before,
                "recovery modified the backup"
            );
        }
    }
}

#[test]
fn replacement_reopen_preserves_recorded_v8_digest_until_journal_finalization() {
    for fail_rename in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let f = fixture(dir.path(), false);
        let before = std::fs::read(&f.path).unwrap();
        let mut ctx = Context::open_existing(&f.path).unwrap();
        if fail_rename {
            assert!(ctx
                .replace_catalogue_file(&dir.path().join("absent-stage"))
                .is_err());
            assert!(
                std::fs::read(&f.path).unwrap() == before,
                "main catalogue bytes changed before recovery"
            );
        } else {
            ctx.replace_catalogue_file(&f.stage).unwrap();
            assert_eq!(icatalog_sync::util::sha256_file(&f.path).unwrap(), f.digest);
        }
        assert_eq!(ctx.catalog().schema_version().unwrap(), 8);
        assert!(ctx.migrations_deferred());
        drop(ctx); // Crash after rename/reopen, before clearing snapshot_ready.
        let mut reopened = Context::open_existing(&f.path).unwrap();
        recover(&mut reopened);
    }
}

#[test]
fn corrupt_or_foreign_companions_defer_migration_and_fail_closed() {
    for damage in ["corrupt", "foreign", "oversized-phase"] {
        let dir = tempfile::tempdir().unwrap();
        let f = fixture(dir.path(), false);
        match damage {
            "corrupt" => std::fs::write(&f.companion, b"not SQLite").unwrap(),
            "foreign" => {
                let state = rusqlite::Connection::open(&f.companion).unwrap();
                state
                    .execute(
                        "UPDATE state_meta SET value='another-catalogue' WHERE key='catalogue-id'",
                        [],
                    )
                    .unwrap();
            }
            _ => {
                let state = rusqlite::Connection::open(&f.companion).unwrap();
                state
                    .execute("UPDATE sync_recovery SET phase=?1", ["x".repeat(40_000)])
                    .unwrap();
            }
        }
        let before = std::fs::read(&f.path).unwrap();
        let mut ctx = Context::open_existing(&f.path).unwrap();
        assert!(ctx.migrations_deferred());
        assert_eq!(ctx.catalog().schema_version().unwrap(), 8);
        let mut params = Params::new();
        params.set("action", "recover");
        assert!(icatalog_sync::dispatch_guarded(&mut ctx, "sync-resolve", &params).is_err());
        assert!(
            std::fs::read(&f.path).unwrap() == before,
            "main catalogue bytes changed before recovery"
        );
        assert!(f.stage.is_file());
    }
}
