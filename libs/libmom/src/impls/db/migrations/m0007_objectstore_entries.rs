use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0007_create_objectstore_entries_table"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS objectstore_entries (
                path TEXT PRIMARY KEY,
                uploaded_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        Ok(())
    }
}
