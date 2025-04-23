use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0005_create_sponsor_table"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sponsors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                sponsors_json TEXT
            )",
            [],
        )?;

        Ok(())
    }
}
