use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0004_github_credentials"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        conn.execute(
            "
   CREATE TABLE github_credentials (
    github_id TEXT NOT NULL,
    data TEXT NOT NULL,
    PRIMARY KEY (github_id)
   )
   ",
            [],
        )?;

        Ok(())
    }
}
