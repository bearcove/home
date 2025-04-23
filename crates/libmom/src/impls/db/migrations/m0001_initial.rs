use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0001_initial"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        conn.execute(
            "
            CREATE TABLE user_preferences (
                id TEXT NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (id)
            )
            ",
            [],
        )?;

        Ok(())
    }
}
