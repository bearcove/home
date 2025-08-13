use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0012_users_vs_profiles"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        // Create new users table without platform-specific columns
        conn.execute(
            "
            CREATE TABLE users_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            ",
            [],
        )?;

        // Create new profile tables with user_id foreign keys
        conn.execute(
            "
            CREATE TABLE github_profiles_new (
                id TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                monthly_usd INTEGER,
                sponsorship_privacy_level STRING,
                name TEXT,
                login TEXT NOT NULL,
                avatar_url TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id),
                FOREIGN KEY (user_id) REFERENCES users(id),
                UNIQUE (user_id)
            )
            ",
            [],
        )?;

        conn.execute(
            "
            CREATE TABLE patreon_profiles_new (
                id TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                tier STRING,
                full_name TEXT NOT NULL,
                avatar_url TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id),
                FOREIGN KEY (user_id) REFERENCES users(id),
                UNIQUE (user_id)
            )
            ",
            [],
        )?;

        conn.execute(
            "
            CREATE TABLE discord_profiles_new (
                id TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                username TEXT NOT NULL,
                global_name TEXT,
                avatar_hash TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id),
                FOREIGN KEY (user_id) REFERENCES users(id),
                UNIQUE (user_id)
            )
            ",
            [],
        )?;

        // Migrate existing data
        // First, copy users and create new user records
        conn.execute(
            "
            INSERT INTO users_new (id, created_at)
            SELECT id, created_at FROM users
            ",
            [],
        )?;

        // Migrate GitHub profiles
        conn.execute(
            "
            INSERT INTO github_profiles_new (id, user_id, monthly_usd, sponsorship_privacy_level, name, login, avatar_url, created_at, updated_at)
            SELECT gp.id, u.id, gp.monthly_usd, gp.sponsorship_privacy_level, gp.name, gp.login, gp.avatar_url, gp.created_at, gp.updated_at
            FROM github_profiles gp
            JOIN users u ON u.github_user_id = gp.id
            WHERE u.github_user_id IS NOT NULL
            ",
            [],
        )?;

        // Migrate Patreon profiles
        conn.execute(
            "
            INSERT INTO patreon_profiles_new (id, user_id, tier, full_name, avatar_url, created_at, updated_at)
            SELECT pp.id, u.id, pp.tier, pp.full_name, pp.avatar_url, pp.created_at, pp.updated_at
            FROM patreon_profiles pp
            JOIN users u ON u.patreon_user_id = pp.id
            WHERE u.patreon_user_id IS NOT NULL
            ",
            [],
        )?;

        // Migrate Discord profiles
        conn.execute(
            "
            INSERT INTO discord_profiles_new (id, user_id, username, global_name, avatar_hash, created_at, updated_at)
            SELECT dp.id, u.id, dp.username, dp.global_name, dp.avatar_hash, dp.created_at, dp.updated_at
            FROM discord_profiles dp
            JOIN users u ON u.discord_user_id = dp.id
            WHERE u.discord_user_id IS NOT NULL
            ",
            [],
        )?;

        // Drop old tables
        conn.execute("DROP TABLE users", [])?;
        conn.execute("DROP TABLE github_profiles", [])?;
        conn.execute("DROP TABLE patreon_profiles", [])?;
        conn.execute("DROP TABLE discord_profiles", [])?;

        // Rename new tables
        conn.execute("ALTER TABLE users_new RENAME TO users", [])?;
        conn.execute(
            "ALTER TABLE github_profiles_new RENAME TO github_profiles",
            [],
        )?;
        conn.execute(
            "ALTER TABLE patreon_profiles_new RENAME TO patreon_profiles",
            [],
        )?;
        conn.execute(
            "ALTER TABLE discord_profiles_new RENAME TO discord_profiles",
            [],
        )?;

        // Create indexes for efficient lookups
        conn.execute(
            "CREATE INDEX idx_github_profiles_user_id ON github_profiles(user_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX idx_patreon_profiles_user_id ON patreon_profiles(user_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX idx_discord_profiles_user_id ON discord_profiles(user_id)",
            [],
        )?;

        Ok(())
    }
}
