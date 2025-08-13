use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0013_add_gifted_tier"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        // Add gifted_tier column to users table
        conn.execute("ALTER TABLE users ADD COLUMN gifted_tier TEXT", [])?;

        Ok(())
    }
}
