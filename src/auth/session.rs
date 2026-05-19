use getrandom::fill;
use sha2::{Digest, Sha256};
use std::fmt;
use subtle::ConstantTimeEq;

pub const SESSION_TOKEN_BYTES: usize = 32;
pub const SESSION_TOKEN_HEX_LEN: usize = SESSION_TOKEN_BYTES * 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSessionToken {
    secret: String,
    token_hash: String,
}

#[derive(Debug)]
pub enum SessionTokenError {
    Random(getrandom::Error),
}

impl NewSessionToken {
    pub fn generate() -> Result<Self, SessionTokenError> {
        let mut bytes = [0_u8; SESSION_TOKEN_BYTES];
        fill(&mut bytes).map_err(SessionTokenError::Random)?;
        let secret = hex_encode(&bytes);
        let token_hash = hash_session_token(&secret);

        Ok(Self { secret, token_hash })
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }
}

pub fn hash_session_token(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hex_encode(&hasher.finalize())
}

pub fn verify_session_token_hash(secret: &str, expected_hash: &str) -> bool {
    let actual_hash = hash_session_token(secret);
    actual_hash
        .as_bytes()
        .ct_eq(expected_hash.as_bytes())
        .into()
}

pub fn plausible_session_token(secret: &str) -> bool {
    secret.len() == SESSION_TOKEN_HEX_LEN && secret.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl fmt::Display for SessionTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Random(err) => write!(formatter, "secure random token generation failed: {err}"),
        }
    }
}

impl std::error::Error for SessionTokenError {}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_session_token_is_random_and_hashable() {
        let first = NewSessionToken::generate().unwrap();
        let second = NewSessionToken::generate().unwrap();

        assert_ne!(first.secret(), second.secret());
        assert_ne!(first.token_hash(), second.token_hash());
        assert!(plausible_session_token(first.secret()));
        assert_eq!(first.secret().len(), SESSION_TOKEN_HEX_LEN);
        assert_eq!(first.token_hash().len(), 64);
    }

    #[test]
    fn token_hash_verifies_without_storing_secret() {
        let token = NewSessionToken::generate().unwrap();

        assert!(verify_session_token_hash(
            token.secret(),
            token.token_hash()
        ));
        assert!(!verify_session_token_hash(
            "0000000000000000000000000000000000000000000000000000000000000000",
            token.token_hash()
        ));
    }

    #[test]
    fn plausible_session_token_rejects_wrong_shape() {
        assert!(!plausible_session_token("short"));
        assert!(!plausible_session_token(
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        ));
    }
}
