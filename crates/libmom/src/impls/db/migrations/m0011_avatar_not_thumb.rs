use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0011_rename_thumb_url_to_avatar_url"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        // Rename thumb_url to avatar_url in github_profiles table
        conn.execute(
            "ALTER TABLE github_profiles RENAME COLUMN thumb_url TO avatar_url",
            [],
        )?;

        // Rename thumb_url to avatar_url in patreon_profiles table
        conn.execute(
            "ALTER TABLE patreon_profiles RENAME COLUMN thumb_url TO avatar_url",
            [],
        )?;

        Ok(())
    }
}
