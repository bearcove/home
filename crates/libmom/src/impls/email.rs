use config_types::EmailConfig;
use eyre::Result;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    message::{Message, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use log::{debug, error, info, trace};

pub struct EmailService {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from_email: String,
    from_name: String,
}

impl EmailService {
    pub fn new(config: &EmailConfig) -> Result<Self> {
        info!(
            "Initializing email service with SMTP host: {}:{}",
            config.smtp_host, config.smtp_port
        );
        debug!(
            "Email service config - from: {} <{}>, username: {}",
            config.from_name, config.from_email, config.smtp_username
        );

        let creds = Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)?
            .port(config.smtp_port)
            .credentials(creds)
            .build();

        info!("Email service initialized successfully");
        Ok(Self {
            mailer,
            from_email: config.from_email.clone(),
            from_name: config.from_name.clone(),
        })
    }

    pub async fn send_login_code(
        &self,
        to_email: &str,
        code: &str,
        tenant_name: &str,
    ) -> Result<()> {
        info!("Preparing to send login code email to {to_email} for tenant {tenant_name}");
        debug!("Login code: {code} (expires in 15 minutes)");

        let subject = format!("Your login code for {tenant_name}");
        trace!("Email subject: {subject}");

        let body = format!(
            r#"Hello!

Your login code is: {code}

This code will expire in 15 minutes.

If you didn't request this code, you can safely ignore this email.

Thanks,
The {tenant_name} team"#,
        );
        trace!("Email body length: {} chars", body.len());

        debug!(
            "Building email message from {} <{}> to {}",
            self.from_name, self.from_email, to_email
        );
        let email = Message::builder()
            .from(format!("{} <{}>", self.from_name, self.from_email).parse()?)
            .to(to_email.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)?;

        info!("Sending email via SMTP...");
        let send_start = std::time::Instant::now();

        match self.mailer.send(email).await {
            Ok(response) => {
                let duration = send_start.elapsed();
                info!("Email sent successfully to {to_email} in {duration:?}");
                debug!("SMTP response: {response:?}");
                Ok(())
            }
            Err(e) => {
                let duration = send_start.elapsed();
                error!(
                    "Failed to send email to {to_email} after {duration:?}: {e}"
                );
                debug!("SMTP error details: {e:?}");
                Err(e.into())
            }
        }
    }
}
