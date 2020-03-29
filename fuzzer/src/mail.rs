use lettre::smtp::authentication::Credentials;
use lettre::smtp::{ClientSecurity, ConnectionReuseParameters, SmtpTransport};
use lettre::{ClientTlsParameters, EmailAddress, Envelope, SmtpClient, Transport};
use lettre_email::EmailBuilder;
use native_tls::TlsConnector;
use serde::Deserialize;
use std::env;
use std::sync::Once;
use tokio::sync::Mutex;

static mut MAILER: Option<Mutex<SmtpTransport>> = None;
static mut ENVELOPE: Option<Envelope> = None;
static ONCE: Once = Once::new();

#[derive(Debug, Deserialize)]
pub struct MailConf {
    pub sender: String,
    pub receivers: Vec<String>,
}

pub async fn send(mail: EmailBuilder) {
    unsafe {
        if let (Some(mailer), Some(envelope)) = (MAILER.as_ref(), ENVELOPE.as_ref()) {
            let mail = mail.envelope(envelope.clone()).build().unwrap();
            {
                let mut mailer = mailer.lock().await;
                mailer.send(mail.into()).unwrap();
            }
        }
    }
}

pub fn init(conf: &MailConf) {
    ONCE.call_once(|| {
        let passwd = env::var("HEALER_MAIL_PASSWD").unwrap();
        let creds = Credentials::new(conf.sender.clone(), passwd);
        let tls = TlsConnector::builder();
        let param = ClientTlsParameters::new("smtp-mail.outlook.com".into(), tls.build().unwrap());
        let mailer = SmtpClient::new(
            ("smtp-mail.outlook.com", 587),
            ClientSecurity::Required(param),
        )
        .unwrap()
        .credentials(creds)
        .connection_reuse(ConnectionReuseParameters::ReuseUnlimited)
        .smtp_utf8(true)
        .transport();

        let envelope = Envelope::new(
            Some(EmailAddress::new(conf.sender.clone()).unwrap()),
            conf.receivers
                .iter()
                .map(|r| EmailAddress::new(r.clone()).unwrap())
                .collect(),
        )
        .unwrap();

        unsafe {
            MAILER = Some(Mutex::new(mailer));
            ENVELOPE = Some(envelope);
        }
    })
}
