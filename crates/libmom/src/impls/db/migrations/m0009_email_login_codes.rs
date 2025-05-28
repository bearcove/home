use rusqlite::Connection;

pub struct Migration;

impl super::SqlMigration for Migration {
    fn tag(&self) -> &'static str {
        "m0009_email_login_codes"
    }

    fn up(&self, conn: &Connection) -> eyre::Result<()> {
        // Create the table
        conn.execute(
            "
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
   )
   ",
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX idx_email_login_codes_email ON email_login_codes(email)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX idx_email_login_codes_code ON email_login_codes(code)",
            [],
        )?;
        
        conn.execute(
            "CREATE INDEX idx_email_login_codes_expires_at ON email_login_codes(expires_at)",
            [],
        )?;

        Ok(())
    }
}