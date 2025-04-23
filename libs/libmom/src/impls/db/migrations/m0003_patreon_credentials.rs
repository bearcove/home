use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0003_patreon_credentials"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        conn.execute(
            "
            CREATE TABLE patreon_credentials (
                patreon_id TEXT NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (patreon_id)
            )
            ",
            [],
        )?;

        Ok(())
    }
}
