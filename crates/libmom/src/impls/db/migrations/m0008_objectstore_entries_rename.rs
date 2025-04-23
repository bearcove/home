use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0008_objectstore_entries_rename"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        conn.execute(
            "ALTER TABLE objectstore_entries RENAME COLUMN path TO key",
            [],
        )?;

        Ok(())
    }
}
