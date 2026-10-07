//! Outgoing mail over SMTP, configured through environment variables:
//!
//!   LEGEJO_SMTP_HOST      e.g. smtp.example.org
//!   LEGEJO_SMTP_PORT      default 587
//!   LEGEJO_SMTP_USER      SMTP username
//!   LEGEJO_SMTP_PASSWORD  SMTP password or provider API key
//!   LEGEJO_SMTP_TLS       starttls (default) | tls | none (tests only)
//!   LEGEJO_MAIL_FROM      e.g. "Legejo <noreply@example.org>"

use lettre::message::header::{ContentTransferEncoding, ContentType, Header, HeaderName, HeaderValue};
use lettre::message::{Mailbox, MultiPart, SinglePart};
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

impl MailConfig {
    /// An unencrypted server on this machine, without credentials.
    #[cfg(test)]
    pub fn local(port: u16, from: &str) -> Self {
        MailConfig { from: from.into(), host: "127.0.0.1".into(), port, credentials: None, tls: Tls::None }
    }

    /// The bare address mail is sent from, without the display name.
    pub fn from_address(&self) -> Option<String> {
        self.from.parse::<Mailbox>().ok().map(|m| m.email.to_string())
    }
}

pub async fn send(config: &MailConfig, to: &str, subject: &str, html: &str) -> anyhow::Result<()> {
    let message = Message::builder()
        .from(config.from.parse()?)
        .to(to.parse()?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html.to_string())?;
    deliver(config, message).await
}

/// `Content-Disposition: attachment` with the file name written out plainly
/// on one line. The library's own header splits the name into numbered
/// pieces, which not every receiving system puts together again.
#[derive(Clone)]
struct PlainAttachment(String);

impl Header for PlainAttachment {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str("Content-Disposition")
    }

    fn parse(s: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(s.to_string()))
    }

    fn display(&self) -> HeaderValue {
        let value = format!("attachment; filename=\"{}\"", self.0);
        HeaderValue::dangerous_new_pre_encoded(Self::name(), value.clone(), value)
    }
}

/// A short text with one file attached. `filename` must be plain ASCII
/// without quotes.
pub async fn send_file(
    config: &MailConfig,
    to: &str,
    subject: &str,
    text: &str,
    filename: &str,
    mime: &str,
    data: Vec<u8>,
) -> anyhow::Result<()> {
    let message = Message::builder().from(config.from.parse()?).to(to.parse()?).subject(subject).multipart(
        MultiPart::mixed()
            .singlepart(SinglePart::plain(text.to_string()))
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::parse(mime)?)
                    .header(PlainAttachment(filename.to_string()))
                    .header(ContentTransferEncoding::Base64)
                    .body(data),
            ),
    )?;
    deliver(config, message).await
}

async fn deliver(config: &MailConfig, message: Message) -> anyhow::Result<()> {
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
