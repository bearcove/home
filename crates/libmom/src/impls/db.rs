mod migrations;

use camino::Utf8Path;
use config::TenantInfo;
use eyre::Result;
use migrations::{SqlMigration, all_migrations};
use r2d2_sqlite::SqliteConnectionManager;
use std::time::Duration;

use crate::impls::Pool;

pub(crate) fn mom_db_pool(ti: &TenantInfo) -> Result<Pool> {
    Ok(Pool(mk_sqlite_pool(&ti.mom_db_file(), all_migrations())?))
}

fn mk_sqlite_pool(
    path: &Utf8Path,
    migrations: Vec<Box<dyn SqlMigration>>,
) -> Result<r2d2::Pool<SqliteConnectionManager>> {
    let manager = SqliteConnectionManager::file(path)
        .with_flags(
            rusqlite::OpenFlags::SQLITE_OPEN_CREATE | rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .with_init(|conn| {
            // Set the journal mode to Write-Ahead Logging for concurrency
            conn.pragma_update(None, "journal_mode", "WAL")?;
            // Set synchronous mode to NORMAL for performance and data safety balance
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            // Set busy timeout to 5 seconds to avoid "database is locked" errors
            conn.pragma_update(None, "busy_timeout", "5000")?;
            // Set cache size to 20MB for faster data access
            conn.pragma_update(None, "cache_size", "-20000")?;
            // Enable foreign key constraint enforcement
            conn.pragma_update(None, "foreign_keys", "ON")?;
            // Enable auto vacuuming and set it to incremental mode for gradual space reclaiming
            conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
            // Store temporary tables and data in memory for better performance
            conn.pragma_update(None, "temp_store", "MEMORY")?;
            // Set the mmap_size to 2GB for faster read/write access using memory-mapped I/O
            conn.pragma_update(None, "mmap_size", "2147483648")?;
            // Set the page size to 8KB for balanced memory usage and performance
            conn.pragma_update(None, "page_size", "8192")?;
            conn.busy_timeout(Duration::from_secs(10))?;
            Ok(())
        });

    let pool = r2d2::Pool::builder()
        .connection_timeout(Duration::from_secs(1))
        .build(manager)?;
    migrations::migrate_all_sqlite(pool.get()?.transaction()?, migrations)?;

    Ok(pool)
}
