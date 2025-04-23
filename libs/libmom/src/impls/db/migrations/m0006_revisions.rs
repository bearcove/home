use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0006_create_revisions_table"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS revisions (
                id TEXT PRIMARY KEY,
                object_key TEXT NOT NULL,
                uploaded_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        Ok(())
    }
}
