use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{COOKIE, SET_COOKIE},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::Parser;
use mythenheim::{
    VERSION,
    auth::service::{
        AuthError, AuthService, PublicUser, expired_session_cookie, extract_session_cookie,
        session_cookie,
    },
    config::AppConfig,
    db::migrations,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};
use tower_http::trace::TraceLayer;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[arg(long, default_value = "examples/mythenheim.toml")]
    config: PathBuf,

    #[arg(long)]
    check_config: bool,

    #[arg(long)]
    check_migrations: bool,

    #[arg(long)]
    print_migrations: bool,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Debug, Clone)]
struct AppState {
    auth: AuthService,
    secure_cookies: bool,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    login: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct AuthUserResponse {
    user: PublicUserResponse,
}

#[derive(Debug, Serialize)]
struct PublicUserResponse {
    id: String,
    username: String,
    trust_level: u8,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if cli.check_migrations {
        migrations::validate(migrations::all())?;
        println!("migrations ok: {} migration(s)", migrations::all().len());
        return Ok(());
    }

    if cli.print_migrations {
        print!("{}", migrations::render_all()?);
        return Ok(());
    }

    let config = AppConfig::load(&cli.config)?;

    if cli.check_config {
        println!("config ok: {}", cli.config.display());
        return Ok(());
    }

    let listen_addr: SocketAddr = config.server.listen_addr.parse()?;
    let max_request_body_bytes = config.security.max_request_body_bytes;
    let secure_cookies = config.security.secure_cookies;
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    tracing::info!(%listen_addr, "starting mythenheim");

    axum::serve(
        listener,
        app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                secure_cookies,
            },
            max_request_body_bytes as usize,
        ),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
fn app() -> Router {
    app_with_state(
        AppState {
            auth: AuthService::new_in_memory(),
            secure_cookies: true,
        },
        1_048_576,
    )
}

fn app_with_state(state: AppState, max_request_body_bytes: usize) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(me))
        .layer(DefaultBodyLimit::max(max_request_body_bytes))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
    })
}

async fn register(State(state): State<AppState>, Json(payload): Json<RegisterRequest>) -> Response {
    match state
        .auth
        .register(&payload.username, &payload.email, &payload.password)
    {
        Ok(user) => (StatusCode::CREATED, Json(AuthUserResponse::from(user))).into_response(),
        Err(err) => auth_error_response(err),
    }
}

async fn login(State(state): State<AppState>, Json(payload): Json<LoginRequest>) -> Response {
    match state.auth.login(&payload.login, &payload.password) {
        Ok(session) => {
            let mut response =
                (StatusCode::OK, Json(AuthUserResponse::from(session.user))).into_response();
            set_cookie(
                response.headers_mut(),
                &session_cookie(&session.session_secret, state.secure_cookies),
            );
            response
        }
        Err(err) => auth_error_response(err),
    }
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session_secret) = session_secret_from_headers(&headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };

    match state.auth.authenticate(&session_secret) {
        Ok(user) => (StatusCode::OK, Json(AuthUserResponse::from(user))).into_response(),
        Err(err) => auth_error_response(err),
    }
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(session_secret) = session_secret_from_headers(&headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };

    match state.auth.logout(&session_secret) {
        Ok(()) => {
            let mut response = StatusCode::NO_CONTENT.into_response();
            set_cookie(
                response.headers_mut(),
                &expired_session_cookie(state.secure_cookies),
            );
            response
        }
        Err(err) => auth_error_response(err),
    }
}

fn session_secret_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(extract_session_cookie)
}

fn set_cookie(headers: &mut HeaderMap, cookie: &str) {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        headers.append(SET_COOKIE, value);
    }
}

fn auth_error_response(err: AuthError) -> Response {
    let status = match &err {
        AuthError::DuplicateUsername | AuthError::DuplicateEmail => StatusCode::CONFLICT,
        AuthError::InvalidCredentials | AuthError::InvalidSession => StatusCode::UNAUTHORIZED,
        AuthError::InvalidUsername | AuthError::InvalidEmail | AuthError::Password(_) => {
            StatusCode::BAD_REQUEST
        }
        AuthError::Token(_) | AuthError::StorePoisoned => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        status,
        Json(ErrorResponse {
            error: err.to_string(),
        }),
    )
        .into_response()
}

impl From<PublicUser> for AuthUserResponse {
    fn from(user: PublicUser) -> Self {
        Self {
            user: PublicUserResponse {
                id: user.id,
                username: user.username,
                trust_level: user.trust_level,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        body::to_bytes,
        http::{Request, StatusCode, header::CONTENT_TYPE},
    };
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_register_login_me_logout_flow() {
        let app = app();
        let register_response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/auth/register",
                json!({
                    "username": "Member",
                    "email": "member@example.test",
                    "password": "correct horse battery staple"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(register_response.status(), StatusCode::CREATED);

        let login_response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/auth/login",
                json!({
                    "login": "member",
                    "password": "correct horse battery staple"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(login_response.status(), StatusCode::OK);
        let cookie = login_response
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));

        let me_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(me_response.status(), StatusCode::OK);
        let body = body_json(me_response).await;
        assert_eq!(body["user"]["username"], "Member");

        let logout_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/logout")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout_response.status(), StatusCode::NO_CONTENT);

        let me_after_logout = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/me")
                    .header(COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(me_after_logout.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_rejects_duplicate_usernames() {
        let app = app();
        let payload = json!({
            "username": "Member",
            "email": "member@example.test",
            "password": "correct horse battery staple"
        });

        let first = app
            .clone()
            .oneshot(json_request("/api/v1/auth/register", payload.clone()))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::CREATED);

        let second = app
            .oneshot(json_request(
                "/api/v1/auth/register",
                json!({
                    "username": "member",
                    "email": "other@example.test",
                    "password": "correct horse battery staple"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn auth_rejects_oversized_body() {
        let response = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                secure_cookies: false,
            },
            32,
        )
        .oneshot(json_request(
            "/api/v1/auth/register",
            json!({
                "username": "Member",
                "email": "member@example.test",
                "password": "correct horse battery staple"
            }),
        ))
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn auth_rejects_malformed_json() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/register")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from("{not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    fn json_request(uri: &str, payload: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap()
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
