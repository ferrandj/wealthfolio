//! Web push: telling the person's browsers and phones that something is due, the app closed or not.
//!
//! A browser that asks for notifications hands over a subscription (an address at its push service and two
//! keys). A message for it is encrypted for those keys (RFC 8291, `aes128gcm`) and posted to that address,
//! signed with this server's own key pair (VAPID, RFC 8292), so the push service knows who sends. The key pair
//! is made once and kept with the secrets; the subscriptions are kept with the settings.

use std::sync::Arc;

use aws_lc_rs::{aead, agreement, hkdf, rand::SystemRandom, signature};
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use wealthfolio_core::reminders::{Reminder, KIND_RATES};
use wealthfolio_core::secrets::SecretStore;
use wealthfolio_core::settings::SettingsServiceTrait;

/// The settings key the subscriptions are kept under.
pub const SUBSCRIPTIONS_KEY: &str = "push_subscriptions";
/// The secret the server's signing key is kept under (PKCS#8, base64).
const VAPID_SECRET: &str = "push_vapid_key";
/// How many devices may be subscribed.
const MAX_SUBSCRIPTIONS: usize = 20;
/// How long a push service keeps a message for a device that is off, in seconds.
const TTL_SECONDS: u32 = 24 * 60 * 60;

#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Failed(String),
}

fn invalid(message: &str) -> PushError {
    PushError::Invalid(message.to_string())
}

fn failed(message: impl std::fmt::Display) -> PushError {
    PushError::Failed(message.to_string())
}

/// A browser's subscription, as it hands it over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    /// Where its push service takes messages for it.
    pub endpoint: String,
    /// Its public key (P-256, uncompressed, base64url).
    pub p256dh: String,
    /// Its authentication secret (16 bytes, base64url).
    pub auth: String,
    /// What the person calls the device ("Chrome on Android"), to tell them apart.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// What a notification says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub title: String,
    pub body: String,
    /// The page it opens.
    pub url: String,
    /// Notifications with the same tag replace each other on the device.
    pub tag: String,
}

/// What became of a sending.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub sent: usize,
    pub failed: usize,
    /// Subscriptions the push service said are gone: they were dropped.
    pub removed: usize,
}

/// base64url, padded or not (browsers give both).
fn unbase64(value: &str) -> Option<Vec<u8>> {
    let value = value.trim();
    URL_SAFE_NO_PAD
        .decode(value.trim_end_matches('='))
        .or_else(|_| URL_SAFE.decode(value))
        .or_else(|_| STANDARD.decode(value))
        .ok()
}

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn derive(salt: &[u8], secret: &[u8], info: &[u8], len: usize) -> Result<Vec<u8>, PushError> {
    let mut out = vec![0u8; len];
    hkdf::Salt::new(hkdf::HKDF_SHA256, salt)
        .extract(secret)
        .expand(&[info], Len(len))
        .and_then(|okm| okm.fill(&mut out))
        .map_err(|_| failed("Key derivation failed"))?;
    Ok(out)
}

/// A message encrypted for a subscription (RFC 8291): the body to post with `Content-Encoding: aes128gcm`.
/// `sender` is the key pair made for this one message, `salt` 16 random bytes.
fn encrypt(
    sender: &agreement::PrivateKey,
    salt: &[u8; 16],
    receiver_public: &[u8],
    auth_secret: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, PushError> {
    if receiver_public.len() != 65 || auth_secret.len() != 16 {
        return Err(invalid("The subscription's keys are not valid"));
    }
    let sender_public = sender
        .compute_public_key()
        .map_err(|_| failed("Could not make a key for the message"))?;
    let sender_public = sender_public.as_ref();
    let shared = agreement::agree(
        sender,
        agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, receiver_public),
        (),
        |secret| Ok(secret.to_vec()),
    )
    .map_err(|()| invalid("The subscription's public key is not valid"))?;

    // the secret of the exchange, bound to both public keys and to the subscription's secret
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(receiver_public);
    key_info.extend_from_slice(sender_public);
    let ikm = derive(auth_secret, &shared, &key_info, 32)?;
    let key = derive(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = derive(salt, &ikm, b"Content-Encoding: nonce\0", 12)?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| failed("Key derivation failed"))?;

    // one record: the text, then the mark of the last record
    let mut record = plaintext.to_vec();
    record.push(2);
    aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &key).map_err(|_| failed("Encryption failed"))?,
    )
    .seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::empty(),
        &mut record,
    )
    .map_err(|_| failed("Encryption failed"))?;

    // salt, record size, the sender's public key, then the record
    let mut body = salt.to_vec();
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(sender_public.len() as u8);
    body.extend_from_slice(sender_public);
    body.extend_from_slice(&record);
    Ok(body)
}

/// The server's signing key: who sends the messages, for the push services.
struct Vapid {
    key: signature::EcdsaKeyPair,
}

impl Vapid {
    fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, PushError> {
        signature::EcdsaKeyPair::from_pkcs8(&signature::ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8)
            .map(|key| Self { key })
            .map_err(|_| failed("The notification key is not readable"))
    }

    /// The public key browsers subscribe with (base64url).
    fn public_key(&self) -> String {
        use signature::KeyPair as _;
        URL_SAFE_NO_PAD.encode(self.key.public_key().as_ref())
    }

    /// The `Authorization` header for a push service (`audience`: its origin), good until `expires` (seconds
    /// since 1970).
    fn authorization(&self, audience: &str, expires: i64) -> Result<String, PushError> {
        let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::json!({
            "aud": audience,
            "exp": expires,
            "sub": "https://wealthfolio.app",
        });
        let unsigned = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
        let signed = self
            .key
            .sign(&SystemRandom::new(), unsigned.as_bytes())
            .map_err(|_| failed("Could not sign the message"))?;
        Ok(format!(
            "vapid t={unsigned}.{}, k={}",
            URL_SAFE_NO_PAD.encode(signed.as_ref()),
            self.public_key()
        ))
    }
}

/// What a reminder says outside the app, in the person's language (French, else English).
pub fn notification_of(reminder: &Reminder, language: &str) -> Notification {
    let french = language.to_lowercase().starts_with("fr");
    let (title, body) = if reminder.kind == KIND_RATES {
        if french {
            (
                "Taux à vérifier".to_string(),
                "Les taux réglementés ont pu changer : confirmez-les.".to_string(),
            )
        } else {
            (
                "Rates to check".to_string(),
                "The regulated rates may have changed: confirm them.".to_string(),
            )
        }
    } else {
        let about = match &reminder.account_name {
            Some(account) => format!("{} — {account}", reminder.label),
            None => reminder.label.clone(),
        };
        let body = match reminder.how_to.as_deref().filter(|h| !h.trim().is_empty()) {
            Some(how) => format!("{about}\n{how}"),
            None => about,
        };
        (
            if french {
                "Document à importer"
            } else {
                "Document to import"
            }
            .to_string(),
            body,
        )
    };
    Notification {
        title,
        body,
        url: "/reminders".to_string(),
        tag: reminder.id.clone(),
    }
}

pub struct PushService {
    settings: Arc<dyn SettingsServiceTrait>,
    secrets: Arc<dyn SecretStore>,
    client: reqwest::Client,
}

impl PushService {
    pub fn new(settings: Arc<dyn SettingsServiceTrait>, secrets: Arc<dyn SecretStore>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            settings,
            secrets,
            client,
        }
    }

    /// The server's signing key, made the first time it is needed.
    fn vapid(&self) -> Result<Vapid, PushError> {
        if let Some(stored) = self.secrets.get_secret(VAPID_SECRET).map_err(failed)? {
            if let Some(pkcs8) = unbase64(&stored) {
                if let Ok(vapid) = Vapid::from_pkcs8(&pkcs8) {
                    return Ok(vapid);
                }
            }
        }
        let pkcs8 = signature::EcdsaKeyPair::generate_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &SystemRandom::new(),
        )
        .map_err(|_| failed("Could not make the notification key"))?;
        self.secrets
            .set_secret(VAPID_SECRET, &STANDARD.encode(pkcs8.as_ref()))
            .map_err(failed)?;
        Vapid::from_pkcs8(pkcs8.as_ref())
    }

    /// The key a browser subscribes with.
    pub fn public_key(&self) -> Result<String, PushError> {
        Ok(self.vapid()?.public_key())
    }

    pub fn subscriptions(&self) -> Vec<Subscription> {
        self.settings
            .get_setting_value(SUBSCRIPTIONS_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    async fn save(&self, subscriptions: &[Subscription]) -> Result<(), PushError> {
        let json = serde_json::to_string(subscriptions).map_err(failed)?;
        self.settings
            .set_setting_value(SUBSCRIPTIONS_KEY, &json)
            .await
            .map_err(failed)
    }

    /// Keep a browser's subscription (the same address again replaces the one kept).
    pub async fn subscribe(&self, mut subscription: Subscription) -> Result<usize, PushError> {
        let endpoint = reqwest::Url::parse(&subscription.endpoint)
            .map_err(|_| invalid("The subscription's address is not valid"))?;
        if endpoint.scheme() != "https" {
            return Err(invalid("A push service is reached over https"));
        }
        let key = unbase64(&subscription.p256dh).unwrap_or_default();
        let auth = unbase64(&subscription.auth).unwrap_or_default();
        if key.len() != 65 || auth.len() != 16 {
            return Err(invalid("The subscription's keys are not valid"));
        }
        subscription.created_at = Some(chrono::Utc::now().to_rfc3339());
        let mut all = self.subscriptions();
        all.retain(|s| s.endpoint != subscription.endpoint);
        all.push(subscription);
        // the oldest devices leave when there are too many
        if all.len() > MAX_SUBSCRIPTIONS {
            let extra = all.len() - MAX_SUBSCRIPTIONS;
            all.drain(..extra);
        }
        self.save(&all).await?;
        Ok(all.len())
    }

    pub async fn unsubscribe(&self, endpoint: &str) -> Result<usize, PushError> {
        let mut all = self.subscriptions();
        all.retain(|s| s.endpoint != endpoint);
        self.save(&all).await?;
        Ok(all.len())
    }

    /// Post one notification to one subscription. `Ok(false)`: the push service says the subscription is gone.
    async fn post(
        &self,
        vapid: &Vapid,
        subscription: &Subscription,
        notification: &Notification,
    ) -> Result<bool, PushError> {
        let endpoint = reqwest::Url::parse(&subscription.endpoint)
            .map_err(|_| invalid("The subscription's address is not valid"))?;
        let receiver = unbase64(&subscription.p256dh).unwrap_or_default();
        let auth = unbase64(&subscription.auth).unwrap_or_default();
        let sender = agreement::PrivateKey::generate(&agreement::ECDH_P256)
            .map_err(|_| failed("Could not make a key for the message"))?;
        let mut salt = [0u8; 16];
        aws_lc_rs::rand::fill(&mut salt).map_err(|_| failed("No randomness"))?;
        let text = serde_json::to_vec(notification).map_err(failed)?;
        let body = encrypt(&sender, &salt, &receiver, &auth, &text)?;
        let authorization = vapid.authorization(
            &endpoint.origin().ascii_serialization(),
            chrono::Utc::now().timestamp() + 12 * 60 * 60,
        )?;
        let response = self
            .client
            .post(endpoint)
            .header("Authorization", authorization)
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("TTL", TTL_SECONDS.to_string())
            .header("Urgency", "normal")
            .body(body)
            .send()
            .await
            .map_err(failed)?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::GONE {
            return Ok(false);
        }
        if !status.is_success() {
            return Err(failed(format!("The push service answered {status}")));
        }
        Ok(true)
    }

    /// Send notifications to every device. The subscriptions that are gone are dropped.
    pub async fn send(&self, notifications: &[Notification]) -> Result<Delivery, PushError> {
        let subscriptions = self.subscriptions();
        let mut delivery = Delivery::default();
        if subscriptions.is_empty() || notifications.is_empty() {
            return Ok(delivery);
        }
        let vapid = self.vapid()?;
        let mut gone: Vec<String> = Vec::new();
        for subscription in &subscriptions {
            for notification in notifications {
                match self.post(&vapid, subscription, notification).await {
                    Ok(true) => delivery.sent += 1,
                    Ok(false) => {
                        gone.push(subscription.endpoint.clone());
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("A notification could not be sent: {e}");
                        delivery.failed += 1;
                    }
                }
            }
        }
        if !gone.is_empty() {
            let kept: Vec<Subscription> = subscriptions
                .into_iter()
                .filter(|s| !gone.contains(&s.endpoint))
                .collect();
            delivery.removed = gone.len();
            self.save(&kept).await?;
        }
        Ok(delivery)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test code")]
    use super::*;

    fn b64(value: &str) -> Vec<u8> {
        unbase64(&value.split_whitespace().collect::<String>()).unwrap()
    }

    /// The example of RFC 8291 (section 5 and appendix A): same keys and salt, same message body.
    #[test]
    fn a_message_is_encrypted_as_the_standard_s_example() {
        let sender = agreement::PrivateKey::from_private_key(
            &agreement::ECDH_P256,
            &b64("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw"),
        )
        .unwrap();
        let receiver = b64(
            "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcx aOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4",
        );
        let auth = b64("BTBZMqHH6r4Tts7J_aSIgg");
        let salt: [u8; 16] = b64("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap();
        let plaintext = b64("V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24");
        assert_eq!(plaintext, b"When I grow up, I want to be a watermelon");

        let body = encrypt(&sender, &salt, &receiver, &auth, &plaintext).unwrap();

        let header = b64(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z 9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml
             mlMoZIIgDll6e3vCYLocInmYWAmS6Tlz AC8wEqKK6PBru3jl7A8",
        );
        let ciphertext = b64(
            "8pfeW0KbunFT06SuDKoJH9Ql87S1QUrd irN6GcG7sFz1y1sqLgVi1VhjVkHsUoEs
             bI_0LpXMuGvnzQ",
        );
        assert_eq!(header.len(), 86);
        assert_eq!(body, [header, ciphertext].concat());
    }

    #[test]
    fn keys_that_are_not_keys_are_refused() {
        let sender = agreement::PrivateKey::generate(&agreement::ECDH_P256).unwrap();
        assert!(encrypt(&sender, &[0; 16], &[4; 10], &[0; 16], b"x").is_err());
        // the right length, but not a point of the curve
        assert!(encrypt(&sender, &[0; 16], &[4; 65], &[0; 16], b"x").is_err());
    }

    /// The signature is checked with the public key the browsers are given.
    #[test]
    fn the_sender_s_signature_is_the_one_of_the_key_browsers_subscribe_with() {
        let pkcs8 = signature::EcdsaKeyPair::generate_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &SystemRandom::new(),
        )
        .unwrap();
        let vapid = Vapid::from_pkcs8(pkcs8.as_ref()).unwrap();
        let header = vapid
            .authorization("https://push.example.net", 1_800_000_000)
            .unwrap();
        let (token, key) = header
            .strip_prefix("vapid t=")
            .unwrap()
            .split_once(", k=")
            .unwrap();
        assert_eq!(key, vapid.public_key());
        let public = unbase64(key).unwrap();
        assert_eq!((public.len(), public[0]), (65, 4));

        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: serde_json::Value =
            serde_json::from_slice(&unbase64(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "https://push.example.net");
        assert_eq!(claims["exp"], 1_800_000_000);
        let signed = unbase64(parts[2]).unwrap();
        assert_eq!(signed.len(), 64);
        signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, &public)
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signed)
            .unwrap();
    }

    #[test]
    fn a_reminder_is_told_in_the_person_s_language() {
        let document = Reminder {
            id: "document:a|x".into(),
            kind: "document".into(),
            due_at: chrono::NaiveDate::from_ymd_opt(2026, 11, 20)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
            due: true,
            label: "Banque · relevé (PDF)".into(),
            account_id: Some("a".into()),
            account_name: Some("Compte courant".into()),
            institution: None,
            how_to: Some("Espace client > Relevés".into()),
            source_key: Some("a|x".into()),
            every_days: Some(30),
        };
        let told = notification_of(&document, "fr");
        assert_eq!(told.title, "Document à importer");
        assert_eq!(
            told.body,
            "Banque · relevé (PDF) — Compte courant\nEspace client > Relevés"
        );
        assert_eq!(
            (told.url.as_str(), told.tag.as_str()),
            ("/reminders", "document:a|x")
        );
        assert_eq!(notification_of(&document, "en").title, "Document to import");
        let rates = Reminder {
            id: "rates".into(),
            kind: "rates".into(),
            how_to: None,
            ..document
        };
        assert_eq!(notification_of(&rates, "fr-FR").title, "Taux à vérifier");
    }
}
