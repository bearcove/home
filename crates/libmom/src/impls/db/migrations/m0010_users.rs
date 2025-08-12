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
                patreon_user_id TEXT,
                github_user_id TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            ",
            [],
        )?;

        // Create the github_profiles table
        conn.execute(
            "
            CREATE TABLE github_profiles (
                id TEXT NOT NULL,
                monthly_usd INTEGER,
                sponsorship_privacy_level STRING,
                name TEXT,
                login TEXT NOT NULL,
                thumb_url TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id)
            )
            ",
            [],
        )?;

        // Create the patreon_profiles table
        conn.execute(
            "
            CREATE TABLE patreon_profiles (
                id TEXT NOT NULL,
                tier STRING,
                full_name TEXT NOT NULL,
                thumb_url TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id)
            )
            ",
            [],
        )?;

        // Drop and recreate github_credentials table
        conn.execute("DROP TABLE IF EXISTS github_credentials", [])?;
        conn.execute(
            "
            CREATE TABLE github_credentials (
                id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                scope TEXT NOT NULL,
                expires_at TIMESTAMP,
                PRIMARY KEY (id)
            )
            ",
            [],
        )?;

        // Drop and recreate patreon_credentials table
        conn.execute("DROP TABLE IF EXISTS patreon_credentials", [])?;
        conn.execute(
            "
            CREATE TABLE patreon_credentials (
                id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                refresh_token TEXT NOT NULL,
                expires_at TIMESTAMP,
                PRIMARY KEY (id)
            )
            ",
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX idx_users_github_user_id ON users(github_user_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX idx_users_patreon_user_id ON users(patreon_user_id)",
            [],
        )?;

        // Create the api_keys table
        conn.execute(
            "
                    CREATE TABLE api_keys (
                        id TEXT NOT NULL,
                        user_id INTEGER NOT NULL,
                        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                        revoked_at TIMESTAMP,
                        PRIMARY KEY (id),
                        FOREIGN KEY (user_id) REFERENCES users(id)
                    )
                    ",
            [],
        )?;

        // Create index for api_keys user_id
        conn.execute("CREATE INDEX idx_api_keys_user_id ON api_keys(user_id)", [])?;

        Ok(())
    }
}
