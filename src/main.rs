use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{COOKIE, RETRY_AFTER, SET_COOKIE},
    },
    response::{IntoResponse, Response},
    routing::{get, patch, post},
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
    forum::{Category, DEFAULT_PAGE_SIZE, ForumError, ForumService, Post, Topic, TopicDetail},
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
    forum: ForumService,
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

#[derive(Debug, Deserialize)]
struct CreateCategoryRequest {
    name: String,
    description: Option<String>,
    parent_id: Option<String>,
    is_private: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateTopicRequest {
    title: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct CreatePostRequest {
    content: String,
}

#[derive(Debug, Deserialize)]
struct EditPostRequest {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ListTopicsQuery {
    page: Option<usize>,
    page_size: Option<usize>,
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
struct CategoriesResponse {
    categories: Vec<CategoryResponse>,
}

#[derive(Debug, Serialize)]
struct CategoryResponse {
    id: String,
    name: String,
    slug: String,
    description: Option<String>,
    parent_id: Option<String>,
    is_locked: bool,
    is_private: bool,
}

#[derive(Debug, Serialize)]
struct TopicsResponse {
    topics: Vec<TopicResponse>,
}

#[derive(Debug, Serialize)]
struct TopicResponse {
    id: String,
    category_id: String,
    author_id: String,
    title: String,
    slug: String,
    reply_count: u32,
    is_locked: bool,
}

#[derive(Debug, Serialize)]
struct TopicDetailResponse {
    topic: TopicResponse,
    posts: Vec<PostResponse>,
}

#[derive(Debug, Serialize)]
struct PostResponse {
    id: String,
    topic_id: String,
    author_id: String,
    content_raw: String,
    content_html: String,
    revision: u32,
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
                forum: ForumService::new_in_memory(),
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
            forum: ForumService::new_in_memory(),
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
        .route(
            "/api/v1/categories",
            get(list_categories).post(create_category),
        )
        .route(
            "/api/v1/categories/{category_id}/topics",
            get(list_topics).post(create_topic),
        )
        .route(
            "/api/v1/topics/{topic_id}",
            get(get_topic).delete(delete_topic),
        )
        .route("/api/v1/topics/{topic_id}/posts", post(create_post))
        .route(
            "/api/v1/posts/{post_id}",
            patch(edit_post).delete(delete_post),
        )
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

async fn list_categories(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let can_read_private = authenticated_user(&state, &headers).is_some();
    match state.forum.list_categories_for(can_read_private) {
        Ok(categories) => Json(CategoriesResponse {
            categories: categories.into_iter().map(CategoryResponse::from).collect(),
        })
        .into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn create_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateCategoryRequest>,
) -> Response {
    if authenticated_user(&state, &headers).is_none() {
        return auth_error_response(AuthError::InvalidSession);
    }

    match state.forum.create_category(
        &payload.name,
        payload.description.as_deref(),
        payload.parent_id.as_deref(),
        payload.is_private.unwrap_or(false),
    ) {
        Ok(category) => {
            (StatusCode::CREATED, Json(CategoryResponse::from(category))).into_response()
        }
        Err(err) => forum_error_response(err),
    }
}

async fn list_topics(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(category_id): axum::extract::Path<String>,
    Query(query): Query<ListTopicsQuery>,
) -> Response {
    let can_read_private = authenticated_user(&state, &headers).is_some();
    match state.forum.list_topics(
        &category_id,
        query.page.unwrap_or(1),
        query.page_size.unwrap_or(DEFAULT_PAGE_SIZE),
        can_read_private,
    ) {
        Ok(topics) => Json(TopicsResponse {
            topics: topics.into_iter().map(TopicResponse::from).collect(),
        })
        .into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn create_topic(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(category_id): axum::extract::Path<String>,
    Json(payload): Json<CreateTopicRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };

    match state
        .forum
        .create_topic(&category_id, &user.id, &payload.title, &payload.content)
    {
        Ok(topic) => (StatusCode::CREATED, Json(TopicDetailResponse::from(topic))).into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn get_topic(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(topic_id): axum::extract::Path<String>,
) -> Response {
    let can_read_private = authenticated_user(&state, &headers).is_some();
    match state.forum.get_topic(&topic_id, can_read_private) {
        Ok(topic) => Json(TopicDetailResponse::from(topic)).into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(topic_id): axum::extract::Path<String>,
    Json(payload): Json<CreatePostRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };

    match state.forum.reply(&topic_id, &user.id, &payload.content) {
        Ok(post) => (StatusCode::CREATED, Json(PostResponse::from(post))).into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn edit_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(post_id): axum::extract::Path<String>,
    Json(payload): Json<EditPostRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };

    match state.forum.edit_post(&post_id, &user.id, &payload.content) {
        Ok(post) => Json(PostResponse::from(post)).into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(post_id): axum::extract::Path<String>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };

    match state.forum.delete_post(&post_id, &user.id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn delete_topic(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(topic_id): axum::extract::Path<String>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };

    match state.forum.delete_topic(&topic_id, &user.id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => forum_error_response(err),
    }
}

fn authenticated_user(state: &AppState, headers: &HeaderMap) -> Option<PublicUser> {
    let session_secret = session_secret_from_headers(headers)?;
    state.auth.authenticate(&session_secret).ok()
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
        AuthError::LoginRateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
        AuthError::InvalidUsername | AuthError::InvalidEmail | AuthError::Password(_) => {
            StatusCode::BAD_REQUEST
        }
        AuthError::Token(_) | AuthError::StorePoisoned => StatusCode::INTERNAL_SERVER_ERROR,
    };

    let mut response = (
        status,
        Json(ErrorResponse {
            error: err.to_string(),
        }),
    )
        .into_response();

    if let AuthError::LoginRateLimited { retry_after_secs } = err
        && let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string())
    {
        response.headers_mut().insert(RETRY_AFTER, value);
    }

    response
}

fn forum_error_response(err: ForumError) -> Response {
    let status = match &err {
        ForumError::InvalidCategoryName
        | ForumError::InvalidTopicTitle
        | ForumError::InvalidPostContent => StatusCode::BAD_REQUEST,
        ForumError::CategoryNotFound | ForumError::TopicNotFound | ForumError::PostNotFound => {
            StatusCode::NOT_FOUND
        }
        ForumError::CategoryLocked | ForumError::TopicLocked => StatusCode::CONFLICT,
        ForumError::Forbidden => StatusCode::FORBIDDEN,
        ForumError::StorePoisoned => StatusCode::INTERNAL_SERVER_ERROR,
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

impl From<Category> for CategoryResponse {
    fn from(category: Category) -> Self {
        Self {
            id: category.id,
            name: category.name,
            slug: category.slug,
            description: category.description,
            parent_id: category.parent_id,
            is_locked: category.is_locked,
            is_private: category.is_private,
        }
    }
}

impl From<Topic> for TopicResponse {
    fn from(topic: Topic) -> Self {
        Self {
            id: topic.id,
            category_id: topic.category_id,
            author_id: topic.author_id,
            title: topic.title,
            slug: topic.slug,
            reply_count: topic.reply_count,
            is_locked: topic.is_locked,
        }
    }
}

impl From<Post> for PostResponse {
    fn from(post: Post) -> Self {
        Self {
            id: post.id,
            topic_id: post.topic_id,
            author_id: post.author_id,
            content_raw: post.content_raw,
            content_html: post.content_html,
            revision: post.revision,
        }
    }
}

impl From<TopicDetail> for TopicDetailResponse {
    fn from(detail: TopicDetail) -> Self {
        Self {
            topic: TopicResponse::from(detail.topic),
            posts: detail.posts.into_iter().map(PostResponse::from).collect(),
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
    use mythenheim::auth::service::LOGIN_FAILURE_LIMIT;
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
    async fn auth_rate_limits_repeated_failed_logins() {
        let app = app();
        let register = app
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
        assert_eq!(register.status(), StatusCode::CREATED);

        for _ in 0..LOGIN_FAILURE_LIMIT {
            let response = app
                .clone()
                .oneshot(json_request(
                    "/api/v1/auth/login",
                    json!({
                        "login": "member",
                        "password": "wrong horse battery staple"
                    }),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }

        let response = app
            .oneshot(json_request(
                "/api/v1/auth/login",
                json!({
                    "login": "member",
                    "password": "correct horse battery staple"
                }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key(RETRY_AFTER));
    }

    #[tokio::test]
    async fn auth_rejects_oversized_body() {
        let response = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
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

    #[tokio::test]
    async fn forum_write_routes_require_authentication() {
        let response = app()
            .oneshot(json_request(
                "/api/v1/categories",
                json!({
                    "name": "General"
                }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn forum_category_topic_reply_flow() {
        let app = app();
        let cookie = register_and_login(&app).await;

        let category_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/categories",
                &cookie,
                json!({
                    "name": "General Talk",
                    "description": "Open community discussion"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(category_response.status(), StatusCode::CREATED);
        let category = body_json(category_response).await;
        assert_eq!(category["slug"], "general-talk");
        let category_id = category["id"].as_str().unwrap();

        let list_categories = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/categories")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_categories.status(), StatusCode::OK);
        let categories = body_json(list_categories).await;
        assert_eq!(categories["categories"].as_array().unwrap().len(), 1);

        let topic_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/categories/{category_id}/topics"),
                &cookie,
                json!({
                    "title": "Welcome Thread",
                    "content": "hello **forum** <script>alert(1)</script>"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(topic_response.status(), StatusCode::CREATED);
        let topic = body_json(topic_response).await;
        assert_eq!(topic["topic"]["slug"], "welcome-thread");
        assert_eq!(topic["posts"].as_array().unwrap().len(), 1);
        assert!(
            topic["posts"][0]["content_html"]
                .as_str()
                .unwrap()
                .contains("<strong>forum</strong>")
        );
        assert!(
            !topic["posts"][0]["content_html"]
                .as_str()
                .unwrap()
                .contains("<script")
        );
        let topic_id = topic["topic"]["id"].as_str().unwrap();

        let reply_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/topics/{topic_id}/posts"),
                &cookie,
                json!({
                    "content": "first reply"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(reply_response.status(), StatusCode::CREATED);
        let reply = body_json(reply_response).await;
        let reply_id = reply["id"].as_str().unwrap();

        let edit_response = app
            .clone()
            .oneshot(method_json_request_with_cookie(
                "PATCH",
                &format!("/api/v1/posts/{reply_id}"),
                &cookie,
                json!({
                    "content": "edited **reply**"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(edit_response.status(), StatusCode::OK);
        let edited = body_json(edit_response).await;
        assert_eq!(edited["revision"], 2);
        assert!(
            edited["content_html"]
                .as_str()
                .unwrap()
                .contains("<strong>reply</strong>")
        );

        let get_topic = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/topics/{topic_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_topic.status(), StatusCode::OK);
        let loaded = body_json(get_topic).await;
        assert_eq!(loaded["topic"]["reply_count"], 1);
        assert_eq!(loaded["posts"].as_array().unwrap().len(), 2);

        let list_topics = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/categories/{category_id}/topics"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_topics.status(), StatusCode::OK);
        let topics = body_json(list_topics).await;
        assert_eq!(topics["topics"].as_array().unwrap().len(), 1);

        let delete_reply = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/posts/{reply_id}"))
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(delete_reply.status(), StatusCode::NO_CONTENT);

        let get_after_delete = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/topics/{topic_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_after_delete.status(), StatusCode::OK);
        let loaded_after_delete = body_json(get_after_delete).await;
        assert_eq!(loaded_after_delete["topic"]["reply_count"], 0);
        assert_eq!(loaded_after_delete["posts"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn private_categories_require_session_for_reads() {
        let app = app();
        let cookie = register_and_login(&app).await;

        let category_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/categories",
                &cookie,
                json!({
                    "name": "Staff",
                    "description": "Private discussion",
                    "is_private": true
                }),
            ))
            .await
            .unwrap();
        assert_eq!(category_response.status(), StatusCode::CREATED);
        let category = body_json(category_response).await;
        assert_eq!(category["is_private"], true);
        let category_id = category["id"].as_str().unwrap();

        let topic_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/categories/{category_id}/topics"),
                &cookie,
                json!({
                    "title": "Staff Topic",
                    "content": "private body"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(topic_response.status(), StatusCode::CREATED);
        let topic = body_json(topic_response).await;
        let topic_id = topic["topic"]["id"].as_str().unwrap();

        let anonymous_categories = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/categories")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous_categories.status(), StatusCode::OK);
        assert_eq!(
            body_json(anonymous_categories).await["categories"]
                .as_array()
                .unwrap()
                .len(),
            0
        );

        let authenticated_categories = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/categories")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated_categories.status(), StatusCode::OK);
        assert_eq!(
            body_json(authenticated_categories).await["categories"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let anonymous_topic = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/topics/{topic_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous_topic.status(), StatusCode::NOT_FOUND);

        let authenticated_topic = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/topics/{topic_id}"))
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated_topic.status(), StatusCode::OK);
    }

    async fn register_and_login(app: &Router) -> String {
        let register_response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/auth/register",
                json!({
                    "username": "ForumMember",
                    "email": "forum-member@example.test",
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
                    "login": "ForumMember",
                    "password": "correct horse battery staple"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(login_response.status(), StatusCode::OK);

        login_response
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    }

    fn json_request(uri: &str, payload: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap()
    }

    fn json_request_with_cookie(
        uri: &str,
        cookie: &str,
        payload: serde_json::Value,
    ) -> Request<Body> {
        method_json_request_with_cookie("POST", uri, cookie, payload)
    }

    fn method_json_request_with_cookie(
        method: &str,
        uri: &str,
        cookie: &str,
        payload: serde_json::Value,
    ) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .header(COOKIE, cookie)
            .body(Body::from(payload.to_string()))
            .unwrap()
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
