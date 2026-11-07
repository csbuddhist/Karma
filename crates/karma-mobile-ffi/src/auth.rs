//! Administrator password, one-time recovery codes, and short-lived sessions.
//! Hashing matches the desktop service: Argon2id with the crate defaults,
//! five failures lock authentication for 30 seconds, sessions last 15 minutes.

use std::collections::HashMap;

use argon2::password_hash::{SaltString, rand_core::OsRng as PasswordOsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rand::Rng;
use rand::RngCore;
use rand::rngs::OsRng;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::store::AuthLimit;
use crate::types::{AdminSession, AuthError};

pub const MIN_PASSWORD_CHARS: usize = 10;
pub const MAX_PASSWORD_CHARS: usize = 256;
pub const MAX_FAILURES: u32 = 5;
pub const FAILURE_COOLDOWN_MS: i64 = 30 * 1000;
pub const SESSION_LIFETIME_MS: i64 = 15 * 60 * 1000;
pub const RECOVERY_CODE_COUNT: usize = 8;
const MAX_SESSIONS: usize = 8;
/// Crockford-style alphabet without easily confused characters.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";

pub fn validate_new_password(password: &str) -> Result<(), AuthError> {
    let count = password.chars().count();
    if !(MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&count) {
        return Err(AuthError::WeakPassword);
    }
    Ok(())
}

pub fn hash_secret(secret: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut PasswordOsRng);
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::Storage)
}

pub fn secret_matches(stored_hash: &str, secret: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(stored_hash).map_err(|_| AuthError::Storage)?;
    Ok(Argon2::default()
        .verify_password(secret.as_bytes(), &parsed)
        .is_ok())
}

/// A new recovery code such as `K7QM-2XTP-9HWD`, about 59 bits of entropy.
pub fn generate_recovery_code() -> Zeroizing<String> {
    let mut rng = OsRng;
    let mut code = Zeroizing::new(String::with_capacity(14));
    for group in 0..3 {
        if group > 0 {
            code.push('-');
        }
        for _ in 0..4 {
            let index = rng.gen_range(0..CODE_ALPHABET.len());
            code.push(CODE_ALPHABET[index] as char);
        }
    }
    code
}

/// The canonical form codes are hashed in: uppercase without separators.
pub fn normalize_recovery_code(code: &str) -> Zeroizing<String> {
    Zeroizing::new(
        code.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase())
            .collect(),
    )
}

pub fn check_limit(limit: &AuthLimit, now_ms: i64) -> Result<(), AuthError> {
    match limit.blocked_until_ms {
        Some(until) if until > now_ms => Err(AuthError::RateLimited {
            retry_after_ms: until - now_ms,
        }),
        _ => Ok(()),
    }
}

/// Records one attempt. Returns whether it locked authentication.
pub fn record_attempt(limit: &mut AuthLimit, valid: bool, now_ms: i64) -> bool {
    if valid {
        *limit = AuthLimit::default();
        return false;
    }
    limit.failures = limit.failures.saturating_add(1);
    if limit.failures >= MAX_FAILURES {
        limit.failures = 0;
        limit.blocked_until_ms = Some(now_ms.saturating_add(FAILURE_COOLDOWN_MS));
        return true;
    }
    false
}

#[derive(Default)]
pub struct Sessions {
    active: HashMap<String, i64>,
}

impl Sessions {
    pub fn issue(&mut self, now_ms: i64) -> AdminSession {
        self.active.retain(|_, expires| *expires > now_ms);
        if self.active.len() >= MAX_SESSIONS {
            if let Some(oldest) = self
                .active
                .iter()
                .min_by_key(|(_, expires)| **expires)
                .map(|(token, _)| token.clone())
            {
                self.active.remove(&oldest);
            }
        }
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let token = bytes
            .iter()
            .fold(String::with_capacity(64), |mut token, byte| {
                use std::fmt::Write as _;
                let _ = write!(token, "{byte:02x}");
                token
            });
        let expires_at_ms = now_ms.saturating_add(SESSION_LIFETIME_MS);
        self.active.insert(token.clone(), expires_at_ms);
        AdminSession {
            token,
            expires_at_ms,
        }
    }

    pub fn authorize(&mut self, token: &str, now_ms: i64) -> Result<(), AuthError> {
        self.active.retain(|_, expires| *expires > now_ms);
        let presented = token.as_bytes();
        let valid = self.active.keys().fold(false, |found, candidate| {
            found | bool::from(candidate.as_bytes().ct_eq(presented))
        });
        if valid {
            Ok(())
        } else {
            Err(AuthError::SessionExpired)
        }
    }

    pub fn revoke(&mut self, token: &str) {
        self.active.remove(token);
    }

    /// Ends every session except `token`.
    pub fn revoke_others(&mut self, token: &str) {
        self.active.retain(|candidate, _| candidate == token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_failures_lock_for_thirty_seconds() {
        let mut limit = AuthLimit::default();
        for _ in 0..4 {
            assert!(!record_attempt(&mut limit, false, 1_000));
        }
        assert!(record_attempt(&mut limit, false, 1_000));
        assert_eq!(
            check_limit(&limit, 2_000),
            Err(AuthError::RateLimited {
                retry_after_ms: 29_000
            })
        );
        assert_eq!(check_limit(&limit, 31_000), Ok(()));
    }

    #[test]
    fn sessions_expire() {
        let mut sessions = Sessions::default();
        let session = sessions.issue(0);
        assert_eq!(sessions.authorize(&session.token, 1), Ok(()));
        assert_eq!(
            sessions.authorize(&session.token, SESSION_LIFETIME_MS),
            Err(AuthError::SessionExpired)
        );
        assert_eq!(
            sessions.authorize("forged", 1),
            Err(AuthError::SessionExpired)
        );
    }

    #[test]
    fn recovery_codes_normalize_and_verify() {
        let code = generate_recovery_code();
        assert_eq!(code.len(), 14);
        let hash = hash_secret(&normalize_recovery_code(&code)).unwrap();
        let typed = code.to_ascii_lowercase().replace('-', " ");
        assert!(secret_matches(&hash, &normalize_recovery_code(&typed)).unwrap());
    }

    #[test]
    fn short_passwords_are_rejected() {
        assert_eq!(validate_new_password("short"), Err(AuthError::WeakPassword));
        assert_eq!(validate_new_password("long-enough-pw"), Ok(()));
    }
}
