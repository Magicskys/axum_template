use crate::config::Config;
use anyhow::Result;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub async fn send_mail_with_config(
    config: Config,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<()> {
    tracing::debug!("Send Mail: {:?}", config);
    let smtp_server = &config.smtp_server;
    let smtp_port = config.smtp_port;
    let smtp_user = &config.smtp_user;
    let smtp_pass = &config.smtp_pass;
    let from = smtp_user;

    let email = Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject(subject)
        .body(body.to_string())?;

    let creds = Credentials::new(smtp_user.to_string(), smtp_pass.to_string());
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_server)?
        .port(smtp_port)
        .credentials(creds)
        .build();

    mailer.send(email).await?;
    Ok(())
}
