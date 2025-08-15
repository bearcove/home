CREATE TABLE migrations (
        tag TEXT,
        migrated_at DATETIME
    );
CREATE TABLE user_preferences (
                id TEXT NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (id)
            );
CREATE TABLE sponsors (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                sponsors_json TEXT
            );
CREATE TABLE sqlite_sequence(name,seq);
CREATE TABLE revisions (
                id TEXT PRIMARY KEY,
                object_key TEXT NOT NULL,
                uploaded_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
CREATE TABLE objectstore_entries (
                key TEXT PRIMARY KEY,
                uploaded_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
CREATE TABLE email_login_codes (
    id TEXT NOT NULL,
    email TEXT NOT NULL,
    code TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    expires_at DATETIME NOT NULL,
    used_at DATETIME,
    ip_address TEXT,
    user_agent TEXT,
    PRIMARY KEY (id)
   );
CREATE INDEX idx_email_login_codes_email ON email_login_codes(email);
CREATE INDEX idx_email_login_codes_code ON email_login_codes(code);
CREATE INDEX idx_email_login_codes_expires_at ON email_login_codes(expires_at);
CREATE TABLE github_credentials (
                id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                scope TEXT NOT NULL,
                expires_at TIMESTAMP,
                PRIMARY KEY (id)
            );
CREATE TABLE patreon_credentials (
                id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                refresh_token TEXT NOT NULL,
                expires_at TIMESTAMP,
                PRIMARY KEY (id)
            );
CREATE TABLE discord_credentials (
                id TEXT NOT NULL,
                access_token TEXT NOT NULL,
                refresh_token TEXT NOT NULL,
                expires_at TIMESTAMP,
                PRIMARY KEY (id)
            );
CREATE TABLE IF NOT EXISTS "users" (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            , gifted_tier TEXT);
CREATE TABLE IF NOT EXISTS "api_keys" (
                id TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                revoked_at TIMESTAMP,
                PRIMARY KEY (id),
                FOREIGN KEY (user_id) REFERENCES "users"(id)
            );
CREATE TABLE IF NOT EXISTS "github_profiles" (
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
                FOREIGN KEY (user_id) REFERENCES "users"(id),
                UNIQUE (user_id)
            );
CREATE TABLE IF NOT EXISTS "patreon_profiles" (
                id TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                tier STRING,
                full_name TEXT NOT NULL,
                avatar_url TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id),
                FOREIGN KEY (user_id) REFERENCES "users"(id),
                UNIQUE (user_id)
            );
CREATE TABLE IF NOT EXISTS "discord_profiles" (
                id TEXT NOT NULL,
                user_id INTEGER NOT NULL,
                username TEXT NOT NULL,
                global_name TEXT,
                avatar_hash TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP,
                PRIMARY KEY (id),
                FOREIGN KEY (user_id) REFERENCES "users"(id),
                UNIQUE (user_id)
            );
CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_github_profiles_user_id ON github_profiles(user_id);
CREATE INDEX idx_patreon_profiles_user_id ON patreon_profiles(user_id);
CREATE INDEX idx_discord_profiles_user_id ON discord_profiles(user_id);
