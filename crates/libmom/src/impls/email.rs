use eyre::Result;
use lettre::{
    message::{header::ContentType, Message},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};
use config_types::EmailConfig;

pub struct EmailService {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from_email: String,
    from_name: String,
}

impl EmailService {
    pub fn new(config: &EmailConfig) -> Result<Self> {
        let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)?
            .port(config.smtp_port)
            .credentials(creds)
            .build();

        Ok(Self {
            mailer,
            from_email: config.from_email.clone(),
            from_name: config.from_name.clone(),
        })
    }

    pub async fn send_login_code(&self, to_email: &str, code: &str, tenant_name: &str) -> Result<()> {
        let subject = format!("Your login code for {}", tenant_name);
        
        let body = format!(
            r#"Hello!

Your login code is: {}

This code will expire in 15 minutes.

If you didn't request this code, you can safely ignore this email.

Thanks,
The {} team"#,
            code, tenant_name
        );

        let email = Message::builder()
            .from(format!("{} <{}>", self.from_name, self.from_email).parse()?)
            .to(to_email.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)?;

        self.mailer.send(email).await?;
        
        Ok(())
    }
}