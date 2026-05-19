use crate::auth::{
    password::{PasswordError, hash_password, verify_password},
    session::{NewSessionToken, SessionTokenError, verify_session_token_hash},
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

pub const SESSION_COOKIE_NAME: &str = "mythenheim_session";
pub const SESSION_TTL_SECS: u64 = 60 * 60 * 24 * 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    pub trust_level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginSession {
    pub user: PublicUser,
    pub session_secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    InvalidUsername,
    InvalidEmail,
    Password(PasswordError),
    DuplicateUsername,
    DuplicateEmail,
    InvalidCredentials,
    InvalidSession,
    Token(String),
    StorePoisoned,
}

#[derive(Debug, Clone)]
pub struct AuthService {
    inner: Arc<Mutex<AuthState>>,
}

#[derive(Debug, Default)]
struct AuthState {
    next_user_id: u64,
    users: HashMap<String, StoredUser>,
    username_index: HashMap<String, String>,
    email_hash_index: HashMap<String, String>,
    sessions: HashMap<String, StoredSession>,
}

#[derive(Debug, Clone)]
struct StoredUser {
    id: String,
    username: String,
    username_normalized: String,
    email_hash: String,
    password_hash: String,
    trust_level: u8,
}

#[derive(Debug, Clone)]
struct StoredSession {
    user_id: String,
    token_hash: String,
    expires_at: SystemTime,
    revoked_at: Option<SystemTime>,
}

impl AuthService {
    pub fn new_in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuthState {
                next_user_id: 1,
                ..AuthState::default()
            })),
        }
    }

    pub fn register(
        &self,
        username: &str,
        email: &str,
        password: &str,
    ) -> Result<PublicUser, AuthError> {
        let username_normalized = normalize_username(username)?;
        let email_normalized = normalize_email(email)?;
        let email_hash = hash_lookup_value(&email_normalized);
        let password_hash = hash_password(password).map_err(AuthError::Password)?;

        let mut state = self.inner.lock().map_err(|_| AuthError::StorePoisoned)?;
        if state.username_index.contains_key(&username_normalized) {
            return Err(AuthError::DuplicateUsername);
        }
        if state.email_hash_index.contains_key(&email_hash) {
            return Err(AuthError::DuplicateEmail);
        }

        let user_id = format!("user:{}", state.next_user_id);
        state.next_user_id += 1;

        let user = StoredUser {
            id: user_id.clone(),
            username: username.trim().to_owned(),
            username_normalized: username_normalized.clone(),
            email_hash: email_hash.clone(),
            password_hash,
            trust_level: 0,
        };
        let public = user.public();

        state
            .username_index
            .insert(username_normalized, user_id.clone());
        state.email_hash_index.insert(email_hash, user_id.clone());
        state.users.insert(user_id, user);

        Ok(public)
    }

    pub fn login(&self, login: &str, password: &str) -> Result<LoginSession, AuthError> {
        let login_normalized = normalize_login(login)?;
        let user = {
            let state = self.inner.lock().map_err(|_| AuthError::StorePoisoned)?;
            let user_id = state
                .username_index
                .get(&login_normalized)
                .or_else(|| {
                    state
                        .email_hash_index
                        .get(&hash_lookup_value(&login_normalized))
                })
                .ok_or(AuthError::InvalidCredentials)?;
            state
                .users
                .get(user_id)
                .cloned()
                .ok_or(AuthError::InvalidCredentials)?
        };

        if !verify_password(password, &user.password_hash) {
            return Err(AuthError::InvalidCredentials);
        }

        let session = NewSessionToken::generate().map_err(token_error)?;
        let stored_session = StoredSession {
            user_id: user.id.clone(),
            token_hash: session.token_hash().to_owned(),
            expires_at: SystemTime::now() + Duration::from_secs(SESSION_TTL_SECS),
            revoked_at: None,
        };

        let mut state = self.inner.lock().map_err(|_| AuthError::StorePoisoned)?;
        state
            .sessions
            .insert(session.token_hash().to_owned(), stored_session);

        Ok(LoginSession {
            user: user.public(),
            session_secret: session.secret().to_owned(),
        })
    }

    pub fn authenticate(&self, session_secret: &str) -> Result<PublicUser, AuthError> {
        let token_hash = crate::auth::session::hash_session_token(session_secret);
        let state = self.inner.lock().map_err(|_| AuthError::StorePoisoned)?;
        let session = state
            .sessions
            .get(&token_hash)
            .ok_or(AuthError::InvalidSession)?;

        if session.revoked_at.is_some()
            || SystemTime::now().duration_since(session.expires_at).is_ok()
            || !verify_session_token_hash(session_secret, &session.token_hash)
        {
            return Err(AuthError::InvalidSession);
        }

        state
            .users
            .get(&session.user_id)
            .map(StoredUser::public)
            .ok_or(AuthError::InvalidSession)
    }

    pub fn logout(&self, session_secret: &str) -> Result<(), AuthError> {
        let token_hash = crate::auth::session::hash_session_token(session_secret);
        let mut state = self.inner.lock().map_err(|_| AuthError::StorePoisoned)?;
        let session = state
            .sessions
            .get_mut(&token_hash)
            .ok_or(AuthError::InvalidSession)?;
        session.revoked_at = Some(SystemTime::now());
        Ok(())
    }
}

impl Default for AuthService {
    fn default() -> Self {
        Self::new_in_memory()
    }
}

impl StoredUser {
    fn public(&self) -> PublicUser {
        debug_assert_eq!(
            self.username_normalized,
            normalize_username(&self.username).unwrap()
        );
        debug_assert_eq!(self.email_hash.len(), 64);
        PublicUser {
            id: self.id.clone(),
            username: self.username.clone(),
            trust_level: self.trust_level,
        }
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUsername => formatter.write_str("invalid username"),
            Self::InvalidEmail => formatter.write_str("invalid email"),
            Self::Password(err) => write!(formatter, "{err}"),
            Self::DuplicateUsername => formatter.write_str("username is already registered"),
            Self::DuplicateEmail => formatter.write_str("email is already registered"),
            Self::InvalidCredentials => formatter.write_str("invalid credentials"),
            Self::InvalidSession => formatter.write_str("invalid session"),
            Self::Token(err) => write!(formatter, "session token error: {err}"),
            Self::StorePoisoned => formatter.write_str("auth store lock is poisoned"),
        }
    }
}

impl std::error::Error for AuthError {}

pub fn session_cookie(session_secret: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE_NAME}={session_secret}; Path=/; HttpOnly{secure}; SameSite=Strict; Max-Age={SESSION_TTL_SECS}"
    )
}

pub fn expired_session_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly{secure}; SameSite=Strict; Max-Age=0")
}

pub fn extract_session_cookie(cookie_header: &str) -> Option<String> {
    cookie_header
        .split(';')
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find_map(|(name, value)| {
            if name == SESSION_COOKIE_NAME && crate::auth::session::plausible_session_token(value) {
                Some(value.to_owned())
            } else {
                None
            }
        })
}

fn normalize_username(username: &str) -> Result<String, AuthError> {
    let trimmed = username.trim();
    let len = trimmed.len();
    let valid = (3..=32).contains(&len)
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if valid {
        Ok(trimmed.to_ascii_lowercase())
    } else {
        Err(AuthError::InvalidUsername)
    }
}

fn normalize_email(email: &str) -> Result<String, AuthError> {
    let trimmed = email.trim().to_ascii_lowercase();
    let valid = trimmed.len() <= 254
        && trimmed.contains('@')
        && !trimmed.starts_with('@')
        && !trimmed.ends_with('@')
        && !trimmed.contains(char::is_whitespace);
    if valid {
        Ok(trimmed)
    } else {
        Err(AuthError::InvalidEmail)
    }
}

fn normalize_login(login: &str) -> Result<String, AuthError> {
    let trimmed = login.trim();
    if trimmed.contains('@') {
        normalize_email(trimmed)
    } else {
        normalize_username(trimmed)
    }
}

fn hash_lookup_value(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    crate::auth::session::hex_encode(&hasher.finalize())
}

fn token_error(err: SessionTokenError) -> AuthError {
    AuthError::Token(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWORD: &str = "correct horse battery staple";

    #[test]
    fn register_login_authenticate_logout_flow() {
        let auth = AuthService::new_in_memory();
        let user = auth
            .register("Eldryoth", "eldryoth@example.test", PASSWORD)
            .unwrap();

        assert_eq!(user.username, "Eldryoth");
        assert_eq!(user.trust_level, 0);

        let login = auth.login("eldryoth", PASSWORD).unwrap();
        assert_eq!(login.user.id, user.id);

        let current = auth.authenticate(&login.session_secret).unwrap();
        assert_eq!(current.id, user.id);

        auth.logout(&login.session_secret).unwrap();
        assert!(matches!(
            auth.authenticate(&login.session_secret),
            Err(AuthError::InvalidSession)
        ));
    }

    #[test]
    fn login_accepts_email() {
        let auth = AuthService::new_in_memory();
        let user = auth
            .register("Member-1", "member@example.test", PASSWORD)
            .unwrap();

        let login = auth.login("MEMBER@example.test", PASSWORD).unwrap();

        assert_eq!(login.user.id, user.id);
    }

    #[test]
    fn duplicate_username_and_email_are_rejected() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", PASSWORD)
            .unwrap();

        assert!(matches!(
            auth.register("member", "other@example.test", PASSWORD),
            Err(AuthError::DuplicateUsername)
        ));
        assert!(matches!(
            auth.register("Other", "MEMBER@example.test", PASSWORD),
            Err(AuthError::DuplicateEmail)
        ));
    }

    #[test]
    fn invalid_password_does_not_login() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", PASSWORD)
            .unwrap();

        assert!(matches!(
            auth.login("member", "wrong horse battery staple"),
            Err(AuthError::InvalidCredentials)
        ));
    }

    #[test]
    fn session_cookie_round_trips() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", PASSWORD)
            .unwrap();
        let login = auth.login("member", PASSWORD).unwrap();
        let cookie = session_cookie(&login.session_secret, true);
        let header = format!("theme=dark; {cookie}; other=value");

        assert_eq!(
            extract_session_cookie(&header).as_deref(),
            Some(login.session_secret.as_str())
        );
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("SameSite=Strict"));
    }
}
