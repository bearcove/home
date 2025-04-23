use eyre::Result;
use rusqlite::{named_params, Connection, Transaction};
use std::collections::HashSet;
use time::OffsetDateTime;
use tracing::info;

macro_rules! include_migrations {
    ($($module:ident),* $(,)?) => {
        $(mod $module;)*

        pub fn all_migrations() -> Vec<Box<dyn SqlMigration>> {
            vec![
                $(Box::new($module::Migration),)*
            ]
        }
    };
}

include_migrations! {
    m0001_initial,
    m0003_patreon_credentials,
    m0004_github_credentials,
    m0005_sponsors,
    m0006_revisions,
    m0007_objectstore_entries,
    m0008_objectstore_entries_rename,
}

pub fn migrate_all_sqlite(
    conn: Transaction<'_>,
    wanted_list: Vec<Box<dyn SqlMigration>>,
) -> Result<()> {
    conn.execute(
        "
    CREATE TABLE IF NOT EXISTS migrations (
        tag TEXT,
        migrated_at DATETIME
    );
    ",
        [],
    )?;

    let mut existing_list: HashSet<String> = Default::default();
    {
        let mut stmt = conn.prepare("SELECT tag FROM migrations")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        for row in rows {
            existing_list.insert(row?);
        }
    }

    for wanted in wanted_list {
        if existing_list.contains(wanted.tag()) {
            continue;
        }

        info!("Applying migration {:?}", wanted.tag());
        wanted.up(&conn)?;

        conn.execute(
            "
            INSERT INTO migrations
            (tag, migrated_at) VALUES
            (:tag, :migrated_at)
            ",
            named_params! {
                ":tag": wanted.tag(),
                ":migrated_at": OffsetDateTime::now_utc(),
            },
        )?;
    }

    conn.commit()?;

    Ok(())
}

pub trait SqlMigration {
    fn tag(&self) -> &'static str;
    fn up(&self, db: &Connection) -> Result<()>;
}
