use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0014_discord_guild_state"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        // Create discord_guilds table
        conn.execute(
            "CREATE TABLE discord_guilds (
                guild_id TEXT PRIMARY KEY,
                approximate_member_count INTEGER,
                approximate_presence_count INTEGER
            )",
            [],
        )?;

        // Create discord_guild_members table
        conn.execute(
            "CREATE TABLE discord_guild_members (
                guild_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                PRIMARY KEY (guild_id, user_id)
            )",
            [],
        )?;

        Ok(())
    }
}
