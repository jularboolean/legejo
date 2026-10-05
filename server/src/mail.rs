//! Outgoing mail over SMTP, configured through environment variables:
//!
//!   LEGEJO_SMTP_HOST      e.g. smtp.example.org
//!   LEGEJO_SMTP_PORT      default 587
//!   LEGEJO_SMTP_USER      SMTP username
//!   LEGEJO_SMTP_PASSWORD  SMTP password or provider API key
//!   LEGEJO_SMTP_TLS       starttls (default) | tls | none (tests only)
//!   LEGEJO_MAIL_FROM      e.g. "Legejo <noreply@example.org>"

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

#[derive(Clone)]
pub struct MailConfig {
    pub from: String,
    host: String,
    port: u16,
    credentials: Option<Credentials>,
    tls: Tls,
}

#[derive(Clone, Copy, PartialEq)]
enum Tls {
    StartTls,
    Tls,
    None,
}

/// Read the configuration once at startup; None means mail is off.
pub fn from_env() -> anyhow::Result<Option<MailConfig>> {
    use crate::settings::var;
    let Some(host) = var("LEGEJO_SMTP_HOST")? else {
        return Ok(None);
    };
    let from = var("LEGEJO_MAIL_FROM")?
        .ok_or_else(|| anyhow::anyhow!("LEGEJO_SMTP_HOST is set but LEGEJO_MAIL_FROM is missing"))?;
    let port: u16 = var("LEGEJO_SMTP_PORT")?.and_then(|p| p.parse().ok()).unwrap_or(587);
    // The password may also come from a file (LEGEJO_SMTP_PASSWORD_FILE).
    let credentials = match var("LEGEJO_SMTP_USER")? {
        Some(user) => Some(Credentials::new(user, var("LEGEJO_SMTP_PASSWORD")?.unwrap_or_default())),
        None => None,
    };
    let tls = match var("LEGEJO_SMTP_TLS")?.as_deref() {
        Some("tls") => Tls::Tls,
        Some("none") => Tls::None,
        _ => Tls::StartTls,
    };
    Ok(Some(MailConfig { from, host, port, credentials, tls }))
}

pub async fn send(config: &MailConfig, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
    let message = Message::builder()
        .from(config.from.parse()?)
        .to(to.parse()?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html.to_string())?;

    let mut builder = match config.tls {
        Tls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)?,
        Tls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)?,
        Tls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host),
    };
    builder = builder.port(config.port);
    if let Some(credentials) = &config.credentials {
        builder = builder.credentials(credentials.clone());
    }
    builder.build().send(message).await?;
    Ok(())
}
