use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0010_users"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        // Create the users table
        conn.execute(
            "
            CREATE TABLE users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                last_seen TIMESTAMP
            )
            ",
            [],
        )?;

        // Add user_id column to github_credentials
        conn.execute(
            "ALTER TABLE github_credentials ADD COLUMN user_id INTEGER REFERENCES users(id)",
            [],
        )?;

        // Add user_id column to patreon_credentials
        conn.execute(
            "ALTER TABLE patreon_credentials ADD COLUMN user_id INTEGER REFERENCES users(id)",
            [],
        )?;

        // Create the kofi_emails table
        conn.execute(
            "
            CREATE TABLE kofi_emails (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT NOT NULL,
                user_id INTEGER REFERENCES users(id),
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            ",
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX idx_github_credentials_user_id ON github_credentials(user_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX idx_patreon_credentials_user_id ON patreon_credentials(user_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX idx_kofi_emails_user_id ON kofi_emails(user_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX idx_kofi_emails_email ON kofi_emails(email)",
            [],
        )?;

        Ok(())
    }
}
