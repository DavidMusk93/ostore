//! RFC 6238 Authenticator TOTP and Session Management.
//!
//! Provides two-factor / one-time password verification for Authenticator apps
//! (Google Authenticator, Apple Passwords, Microsoft Authenticator, 1Password)
//! and generates 14-day persistent sessions.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha1::Sha1;
use tokio::sync::RwLock;

type HmacSha1 = Hmac<Sha1>;

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32_encode(data: &[u8]) -> String {
    let mut result = String::new();
    let mut buffer: u32 = 0;
    let mut bits_left = 0;

    for &b in data {
        buffer = (buffer << 8) | (b as u32);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let index = ((buffer >> bits_left) & 0x1F) as usize;
            result.push(BASE32_ALPHABET[index] as char);
        }
    }

    if bits_left > 0 {
        let index = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        result.push(BASE32_ALPHABET[index] as char);
    }

    result
}

pub fn base32_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_left = 0;

    for c in s.chars() {
        if c.is_whitespace() || c == '=' {
            continue;
        }
        let val = match c.to_ascii_uppercase() {
            'A'..='Z' => (c.to_ascii_uppercase() as u8 - b'A') as u32,
            '2'..='7' => (c as u8 - b'2' + 26) as u32,
            _ => return Err(format!("invalid base32 char: {c}")),
        };
        buffer = (buffer << 5) | val;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            result.push((buffer >> bits_left) as u8);
        }
    }

    Ok(result)
}

/// Compute 6-digit RFC 6238 TOTP for a given counter.
fn compute_totp(secret: &[u8], time_step: u64) -> Option<String> {
    let mut mac = HmacSha1::new_from_slice(secret).ok()?;
    mac.update(&time_step.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[19] & 0x0f) as usize;
    let code = (((result[offset] & 0x7f) as u32) << 24)
        | (((result[offset + 1]) as u32) << 16)
        | (((result[offset + 2]) as u32) << 8)
        | ((result[offset + 3]) as u32);

    let pin = code % 1_000_000;
    Some(format!("{:06}", pin))
}

pub struct TotpAuth {
    secret_bytes: Vec<u8>,
    secret_base32: String,
}

impl TotpAuth {
    pub fn load_or_generate(root: &Path) -> std::io::Result<Self> {
        let secret_file = root.join(".ostore").join("totp.secret");

        // Try reading existing secret
        if let Ok(content) = std::fs::read_to_string(&secret_file) {
            let trimmed = content.trim().to_uppercase();
            if let Ok(bytes) = base32_decode(&trimmed) {
                if !bytes.is_empty() {
                    return Ok(Self {
                        secret_bytes: bytes,
                        secret_base32: trimmed,
                    });
                }
            }
        }

        // Generate random 20-byte secret (160 bits standard for TOTP)
        use sha2::Digest;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let pid = std::process::id();
        let mut hasher = sha2::Sha256::new();
        hasher.update(now.to_be_bytes());
        hasher.update(pid.to_be_bytes());
        hasher.update(root.as_os_str().as_encoded_bytes());
        let hash = hasher.finalize();
        let secret_bytes = hash[..20].to_vec();
        let secret_base32 = base32_encode(&secret_bytes);

        // Save to file (0600 permissions on unix)
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .mode(0o600)
                .open(&secret_file)?;
            file.write_all(secret_base32.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&secret_file, secret_base32.as_bytes())?;
        }

        tracing::info!(path = %secret_file.display(), "generated new Authenticator TOTP secret");
        Ok(Self {
            secret_bytes,
            secret_base32,
        })
    }

    /// Verify 6-digit OTP against current, -1, and +1 time windows (90s tolerance).
    pub fn verify(&self, code: &str) -> bool {
        let code = code.trim();
        if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };
        let current_step = duration.as_secs() / 30;

        for step in [current_step, current_step.saturating_sub(1), current_step + 1] {
            if let Some(expected) = compute_totp(&self.secret_bytes, step) {
                if expected == code {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_secret_base32(&self) -> &str {
        &self.secret_base32
    }

    pub fn get_otpauth_uri(&self, issuer: &str, account: &str) -> String {
        format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits=6&period=30",
            issuer, account, self.secret_base32, issuer
        )
    }
}

pub struct SessionStore {
    sessions: RwLock<HashMap<String, Instant>>,
    duration: Duration,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(Duration::from_secs(14 * 86400)) // 14 days
    }
}

impl SessionStore {
    pub fn new(duration: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            duration,
        }
    }

    pub async fn create_session(&self) -> String {
        use sha2::Digest;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let rnd = std::process::id();
        let mut hasher = sha2::Sha256::new();
        hasher.update(now.to_be_bytes());
        hasher.update(rnd.to_be_bytes());
        let token = format!("sess_{}", hex::encode(&hasher.finalize()[..16]));

        let mut lock = self.sessions.write().await;
        lock.insert(token.clone(), Instant::now() + self.duration);
        token
    }

    pub async fn verify_session(&self, token: &str) -> bool {
        let mut lock = self.sessions.write().await;
        if let Some(expires_at) = lock.get(token) {
            if Instant::now() < *expires_at {
                return true;
            } else {
                lock.remove(token);
            }
        }
        false
    }

    pub async fn revoke_session(&self, token: &str) {
        let mut lock = self.sessions.write().await;
        lock.remove(token);
    }
}
