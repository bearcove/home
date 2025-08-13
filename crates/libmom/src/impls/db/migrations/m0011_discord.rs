use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0011_discord"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        // Create the discord_profiles table
        // cf. https://discord.com/developers/docs/resources/user
        conn.execute(
            "
            CREATE TABLE discord_profiles (
                id TEXT NOT NULL,
                username TEXT NOT NULL,
                global_name TEXT,
                avatar_hash TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id)
            )
            ",
            [],
        )?;

        // Create the discord_credentials table
        conn.execute(
            "
            CREATE TABLE discord_credentials (
                id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                refresh_token TEXT NOT NULL,
                expires_at TIMESTAMP,
                PRIMARY KEY (id)
            )
            ",
            [],
        )?;

        Ok(())
    }
}
