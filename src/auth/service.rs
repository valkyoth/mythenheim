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
pub const LOGIN_FAILURE_LIMIT: u32 = 5;
pub const LOGIN_LOCKOUT_SECS: u64 = 15 * 60;

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
    LoginRateLimited { retry_after_secs: u64 },
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
    login_failures: HashMap<String, LoginFailure>,
    dummy_password_hash: String,
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

#[derive(Debug, Clone)]
struct LoginFailure {
    count: u32,
    locked_until: Option<SystemTime>,
}

impl AuthService {
    pub fn new_in_memory() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuthState {
                next_user_id: 1,
                dummy_password_hash: generate_dummy_password_hash()
                    .expect("random dummy password material satisfies password policy"),
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
        let login_key = login_lookup_key(&login_normalized);
        let now = SystemTime::now();
        let user = {
            let state = self.inner.lock().map_err(|_| AuthError::StorePoisoned)?;
            reject_locked_login(&state, &login_key, now)?;
            let user_id = state
                .username_index
                .get(&login_normalized)
                .or_else(|| {
                    state
                        .email_hash_index
                        .get(&hash_lookup_value(&login_normalized))
                })
                .cloned();
            let Some(user_id) = user_id else {
                let dummy_password_hash = state.dummy_password_hash.clone();
                drop(state);
                let _ = verify_password(password, &dummy_password_hash);
                self.record_failed_login(&login_key)?;
                return Err(AuthError::InvalidCredentials);
            };
            state
                .users
                .get(&user_id)
                .cloned()
                .ok_or(AuthError::InvalidCredentials)?
        };

        if !verify_password(password, &user.password_hash) {
            self.record_failed_login(&login_key)?;
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
        state.login_failures.remove(&login_key);
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

    fn record_failed_login(&self, login_key: &str) -> Result<(), AuthError> {
        let mut state = self.inner.lock().map_err(|_| AuthError::StorePoisoned)?;
        let failure = state
            .login_failures
            .entry(login_key.to_owned())
            .or_insert(LoginFailure {
                count: 0,
                locked_until: None,
            });
        failure.count = failure.count.saturating_add(1);
        if failure.count >= LOGIN_FAILURE_LIMIT {
            failure.locked_until =
                Some(SystemTime::now() + Duration::from_secs(LOGIN_LOCKOUT_SECS));
        }
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
            Self::LoginRateLimited { retry_after_secs } => {
                write!(
                    formatter,
                    "too many login attempts; retry after {retry_after_secs} seconds"
                )
            }
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

fn login_lookup_key(login_normalized: &str) -> String {
    if login_normalized.contains('@') {
        format!("email:{}", hash_lookup_value(login_normalized))
    } else {
        format!("username:{login_normalized}")
    }
}

fn reject_locked_login(
    state: &AuthState,
    login_key: &str,
    now: SystemTime,
) -> Result<(), AuthError> {
    let Some(failure) = state.login_failures.get(login_key) else {
        return Ok(());
    };
    let Some(locked_until) = failure.locked_until else {
        return Ok(());
    };

    match locked_until.duration_since(now) {
        Ok(remaining) => Err(AuthError::LoginRateLimited {
            retry_after_secs: remaining.as_secs().max(1),
        }),
        Err(_) => Ok(()),
    }
}

fn token_error(err: SessionTokenError) -> AuthError {
    AuthError::Token(err.to_string())
}

fn generate_dummy_password_hash() -> Result<String, AuthError> {
    let dummy_material = NewSessionToken::generate().map_err(token_error)?;
    hash_password(dummy_material.secret()).map_err(AuthError::Password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_login_authenticate_logout_flow() {
        let auth = AuthService::new_in_memory();
        let user = auth
            .register("Eldryoth", "eldryoth@example.test", &test_password())
            .unwrap();

        assert_eq!(user.username, "Eldryoth");
        assert_eq!(user.trust_level, 0);

        let login = auth.login("eldryoth", &test_password()).unwrap();
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
            .register("Member-1", "member@example.test", &test_password())
            .unwrap();

        let login = auth.login("MEMBER@example.test", &test_password()).unwrap();

        assert_eq!(login.user.id, user.id);
    }

    #[test]
    fn duplicate_username_and_email_are_rejected() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", &test_password())
            .unwrap();

        assert!(matches!(
            auth.register("member", "other@example.test", &test_password()),
            Err(AuthError::DuplicateUsername)
        ));
        assert!(matches!(
            auth.register("Other", "MEMBER@example.test", &test_password()),
            Err(AuthError::DuplicateEmail)
        ));
    }

    #[test]
    fn invalid_password_does_not_login() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", &test_password())
            .unwrap();

        assert!(matches!(
            auth.login("member", &wrong_test_password()),
            Err(AuthError::InvalidCredentials)
        ));
    }

    #[test]
    fn repeated_failed_logins_are_rate_limited() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", &test_password())
            .unwrap();

        for _ in 0..LOGIN_FAILURE_LIMIT {
            assert!(matches!(
                auth.login("member", &wrong_test_password()),
                Err(AuthError::InvalidCredentials)
            ));
        }

        assert!(matches!(
            auth.login("member", &test_password()),
            Err(AuthError::LoginRateLimited { retry_after_secs })
                if retry_after_secs > 0 && retry_after_secs <= LOGIN_LOCKOUT_SECS
        ));
    }

    #[test]
    fn unknown_login_attempts_are_rate_limited_without_user_lookup() {
        let auth = AuthService::new_in_memory();

        for _ in 0..LOGIN_FAILURE_LIMIT {
            assert!(matches!(
                auth.login("missing-member", &wrong_test_password()),
                Err(AuthError::InvalidCredentials)
            ));
        }

        assert!(matches!(
            auth.login("missing-member", &wrong_test_password()),
            Err(AuthError::LoginRateLimited { retry_after_secs })
                if retry_after_secs > 0 && retry_after_secs <= LOGIN_LOCKOUT_SECS
        ));
    }

    #[test]
    fn auth_store_has_dummy_password_hash_for_unknown_logins() {
        let auth = AuthService::new_in_memory();
        let state = auth.inner.lock().unwrap();

        assert!(state.dummy_password_hash.starts_with("$argon2id$"));
        assert!(!verify_password(
            &test_password(),
            &state.dummy_password_hash
        ));
    }

    #[test]
    fn successful_login_clears_failed_attempts() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", &test_password())
            .unwrap();

        assert!(matches!(
            auth.login("member", &wrong_test_password()),
            Err(AuthError::InvalidCredentials)
        ));
        assert!(auth.login("member", &test_password()).is_ok());

        for _ in 1..LOGIN_FAILURE_LIMIT {
            assert!(matches!(
                auth.login("member", &wrong_test_password()),
                Err(AuthError::InvalidCredentials)
            ));
        }
        assert!(auth.login("member", &test_password()).is_ok());
    }

    #[test]
    fn session_cookie_round_trips() {
        let auth = AuthService::new_in_memory();
        auth.register("Member", "member@example.test", &test_password())
            .unwrap();
        let login = auth.login("member", &test_password()).unwrap();
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

    fn test_password() -> String {
        "a".repeat(32)
    }

    fn wrong_test_password() -> String {
        "b".repeat(32)
    }
}
