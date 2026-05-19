use argon2::{
    Argon2,
    password_hash::{
        Error as PasswordHashError, PasswordHasher, PasswordVerifier, phc::PasswordHash,
    },
};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordPolicy {
    pub min_bytes: usize,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordError {
    TooShort {
        min_bytes: usize,
        actual_bytes: usize,
    },
    TooLong {
        max_bytes: usize,
        actual_bytes: usize,
    },
    ContainsNul,
    Hash(String),
}

impl PasswordPolicy {
    pub fn validate(&self, password: &str) -> Result<(), PasswordError> {
        let actual_bytes = password.len();
        if actual_bytes < self.min_bytes {
            return Err(PasswordError::TooShort {
                min_bytes: self.min_bytes,
                actual_bytes,
            });
        }
        if actual_bytes > self.max_bytes {
            return Err(PasswordError::TooLong {
                max_bytes: self.max_bytes,
                actual_bytes,
            });
        }
        if password.as_bytes().contains(&0) {
            return Err(PasswordError::ContainsNul);
        }

        Ok(())
    }
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_bytes: 12,
            max_bytes: 1024,
        }
    }
}

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    hash_password_with_policy(password, PasswordPolicy::default())
}

pub fn hash_password_with_policy(
    password: &str,
    policy: PasswordPolicy,
) -> Result<String, PasswordError> {
    policy.validate(password)?;

    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(password_hash_error)
}

pub fn verify_password(password: &str, encoded_hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(encoded_hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

impl fmt::Display for PasswordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort {
                min_bytes,
                actual_bytes,
            } => write!(
                formatter,
                "password is too short: expected at least {min_bytes} bytes, got {actual_bytes}"
            ),
            Self::TooLong {
                max_bytes,
                actual_bytes,
            } => write!(
                formatter,
                "password is too long: expected at most {max_bytes} bytes, got {actual_bytes}"
            ),
            Self::ContainsNul => formatter.write_str("password contains a NUL byte"),
            Self::Hash(err) => write!(formatter, "password hashing failed: {err}"),
        }
    }
}

impl std::error::Error for PasswordError {}

fn password_hash_error(err: PasswordHashError) -> PasswordError {
    PasswordError::Hash(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_policy_rejects_short_passwords() {
        let err = PasswordPolicy::default().validate("short").unwrap_err();

        assert!(matches!(err, PasswordError::TooShort { .. }));
    }

    #[test]
    fn password_policy_rejects_oversized_passwords() {
        let password = "a".repeat(1025);
        let err = PasswordPolicy::default().validate(&password).unwrap_err();

        assert!(matches!(err, PasswordError::TooLong { .. }));
    }

    #[test]
    fn password_policy_rejects_nul_bytes() {
        let err = PasswordPolicy::default()
            .validate("valid-prefix\0valid-suffix")
            .unwrap_err();

        assert_eq!(err, PasswordError::ContainsNul);
    }

    #[test]
    fn hashes_and_verifies_passwords() {
        let password = "correct horse battery staple";
        let hash = hash_password(password).unwrap();

        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong horse battery staple", &hash));
    }

    #[test]
    fn invalid_hash_does_not_verify() {
        assert!(!verify_password(
            "correct horse battery staple",
            "not-a-phc-hash"
        ));
    }
}
