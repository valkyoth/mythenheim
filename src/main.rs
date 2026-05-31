use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{COOKIE, RETRY_AFTER, SET_COOKIE},
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
    forum::{
        Category, CategoryNode, DEFAULT_PAGE_SIZE, ForumError, ForumService, Post, Topic,
        TopicDetail,
    },
    moderation::{
        ApprovalItem, AuditAction, AuditEvent, JobRunSummary, JobStatus, MacroExecution,
        ModerationError, ModerationJob, ModerationMacroAction, ModerationService, QueueStatus,
        Report, StoredModerationMacro, UserModerationState, Warning,
    },
    permissions::{
        ActorPermissions, Capability, PermissionContext, PermissionService, Role, TrustLevel,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, net::SocketAddr, path::PathBuf};
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
    moderation: ModerationService,
    permissions: PermissionService,
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
struct ReportPostRequest {
    reason: String,
}

#[derive(Debug, Deserialize)]
struct IssueWarningRequest {
    target_user_id: String,
    target_id: Option<String>,
    reason: String,
    points: u32,
}

#[derive(Debug, Deserialize)]
struct SetShadowbanRequest {
    shadowbanned: bool,
}

#[derive(Debug, Deserialize)]
struct ResolveQueueItemRequest {
    resolution: String,
}

#[derive(Debug, Deserialize)]
struct ExecuteModerationMacroRequest {
    actions: Vec<ModerationMacroActionRequest>,
}

#[derive(Debug, Deserialize)]
struct CreateModerationMacroRequest {
    name: String,
    description: Option<String>,
    actions: Vec<ModerationMacroActionRequest>,
}

#[derive(Debug, Deserialize)]
struct ScheduleModerationJobRequest {
    run_at_tick: u64,
    actions: Vec<ModerationMacroActionRequest>,
}

#[derive(Debug, Deserialize)]
struct RunDueModerationJobsRequest {
    now_tick: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ModerationMacroActionRequest {
    ResolveReport {
        report_id: String,
        resolution: String,
    },
    ResolveApproval {
        approval_id: String,
        resolution: String,
    },
    IssueWarning {
        target_user_id: String,
        target_id: Option<String>,
        reason: String,
        points: u32,
    },
    ExpireWarning {
        warning_id: String,
        reason: String,
    },
    SetShadowban {
        target_user_id: String,
        shadowbanned: bool,
    },
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
struct CategoryTreeResponse {
    categories: Vec<CategoryNodeResponse>,
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
struct CategoryNodeResponse {
    category: CategoryResponse,
    children: Vec<CategoryNodeResponse>,
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
struct ReportsResponse {
    reports: Vec<ReportResponse>,
}

#[derive(Debug, Serialize)]
struct ReportResponse {
    id: String,
    reporter_id: String,
    target_id: String,
    reason: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ApprovalQueueResponse {
    approvals: Vec<ApprovalItemResponse>,
}

#[derive(Debug, Serialize)]
struct ApprovalItemResponse {
    id: String,
    author_id: String,
    target_id: String,
    reason: String,
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct WarningResponse {
    id: String,
    actor_id: String,
    target_user_id: String,
    target_id: Option<String>,
    reason: String,
    points: u32,
    active: bool,
}

#[derive(Debug, Serialize)]
struct WarningIssuedResponse {
    warning: WarningResponse,
    user_state: UserModerationStateResponse,
}

#[derive(Debug, Serialize)]
struct UserModerationStateResponse {
    active_warning_points: u32,
    muted: bool,
    banned: bool,
    shadowbanned: bool,
}

#[derive(Debug, Serialize)]
struct AuditEventsResponse {
    events: Vec<AuditEventResponse>,
}

#[derive(Debug, Serialize)]
struct AuditEventResponse {
    id: String,
    actor_id: String,
    action: &'static str,
    target_id: String,
    previous_state: Option<UserModerationStateResponse>,
    new_state: Option<UserModerationStateResponse>,
    detail: String,
}

#[derive(Debug, Serialize)]
struct MacroExecutionResponse {
    action_count: usize,
    audit_event_count: usize,
}

#[derive(Debug, Serialize)]
struct StoredModerationMacrosResponse {
    macros: Vec<StoredModerationMacroResponse>,
}

#[derive(Debug, Serialize)]
struct StoredModerationMacroResponse {
    id: String,
    name: String,
    description: Option<String>,
    created_by: String,
    actions: Vec<ModerationMacroActionResponse>,
}

#[derive(Debug, Serialize)]
struct ModerationJobResponse {
    id: String,
    actor_id: String,
    run_at_tick: u64,
    status: &'static str,
    actions: Vec<ModerationMacroActionResponse>,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ModerationMacroActionResponse {
    ResolveReport {
        report_id: String,
        resolution: String,
    },
    ResolveApproval {
        approval_id: String,
        resolution: String,
    },
    IssueWarning {
        target_user_id: String,
        target_id: Option<String>,
        reason: String,
        points: u32,
    },
    ExpireWarning {
        warning_id: String,
        reason: String,
    },
    SetShadowban {
        target_user_id: String,
        shadowbanned: bool,
    },
}

#[derive(Debug, Serialize)]
struct JobRunSummaryResponse {
    checked: usize,
    completed: usize,
    failed: usize,
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
                moderation: ModerationService::new_in_memory(),
                permissions: preview_permission_service(),
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
            moderation: ModerationService::new_in_memory(),
            permissions: preview_permission_service(),
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
        .route("/api/v1/categories/tree", get(category_tree))
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
            get(get_post).patch(edit_post).delete(delete_post),
        )
        .route("/api/v1/posts/{post_id}/reports", post(report_post))
        .route("/api/v1/moderation/reports", get(list_reports))
        .route(
            "/api/v1/moderation/reports/{report_id}/resolve",
            post(resolve_report),
        )
        .route("/api/v1/moderation/approvals", get(list_approvals))
        .route(
            "/api/v1/moderation/approvals/{approval_id}/resolve",
            post(resolve_approval),
        )
        .route("/api/v1/moderation/warnings", post(issue_warning))
        .route(
            "/api/v1/moderation/warnings/{warning_id}/expire",
            post(expire_warning),
        )
        .route("/api/v1/moderation/macros/execute", post(execute_macro))
        .route(
            "/api/v1/moderation/macros",
            get(list_stored_macros).post(create_stored_macro),
        )
        .route(
            "/api/v1/moderation/macros/{macro_id}",
            get(get_stored_macro),
        )
        .route(
            "/api/v1/moderation/macros/{macro_id}/execute",
            post(execute_stored_macro),
        )
        .route("/api/v1/moderation/jobs", post(schedule_job))
        .route("/api/v1/moderation/jobs/run-due", post(run_due_jobs))
        .route("/api/v1/moderation/jobs/{job_id}", get(get_job))
        .route("/api/v1/moderation/audit", get(list_audit_events))
        .route(
            "/api/v1/moderation/users/{user_id}/shadowban",
            post(set_shadowban),
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
    let viewer = authenticated_user(&state, &headers);
    let can_read_private = can_read_private_categories_for(&state, viewer.as_ref());
    match state.forum.list_categories_for(can_read_private) {
        Ok(categories) => Json(CategoriesResponse {
            categories: categories.into_iter().map(CategoryResponse::from).collect(),
        })
        .into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn category_tree(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let viewer = authenticated_user(&state, &headers);
    let can_read_private = can_read_private_categories_for(&state, viewer.as_ref());
    match state.forum.category_tree_for(can_read_private) {
        Ok(categories) => Json(CategoryTreeResponse {
            categories: categories
                .into_iter()
                .map(CategoryNodeResponse::from)
                .collect(),
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
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "category.create", None, None) {
        return forum_error_response(ForumError::Forbidden);
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
    let viewer = authenticated_user(&state, &headers);
    let can_read_private = can_read_private_categories_for(&state, viewer.as_ref());
    let hidden_author_ids = hidden_author_ids(&state);
    match state.forum.list_topics_visible(
        &category_id,
        query.page.unwrap_or(1),
        query.page_size.unwrap_or(DEFAULT_PAGE_SIZE),
        can_read_private,
        viewer.as_ref().map(|user| user.id.as_str()),
        &hidden_author_ids,
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
    if !has_capability(&state, &user, "topic.create", None, Some(&category_id)) {
        return forum_error_response(ForumError::Forbidden);
    }

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
    let viewer = authenticated_user(&state, &headers);
    let can_read_private = can_read_private_categories_for(&state, viewer.as_ref());
    let hidden_author_ids = hidden_author_ids(&state);
    match state.forum.get_topic_visible(
        &topic_id,
        can_read_private,
        viewer.as_ref().map(|user| user.id.as_str()),
        &hidden_author_ids,
    ) {
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
    let Ok(topic_target) = state.forum.topic_permission_target(&topic_id) else {
        return forum_error_response(ForumError::TopicNotFound);
    };
    if !has_capability(
        &state,
        &user,
        "post.reply",
        None,
        Some(&topic_target.category_id),
    ) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.forum.reply(&topic_id, &user.id, &payload.content) {
        Ok(post) => (StatusCode::CREATED, Json(PostResponse::from(post))).into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn get_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(post_id): axum::extract::Path<String>,
) -> Response {
    let viewer = authenticated_user(&state, &headers);
    let can_read_private = can_read_private_categories_for(&state, viewer.as_ref());
    let hidden_author_ids = hidden_author_ids(&state);
    match state.forum.get_post_visible(
        &post_id,
        can_read_private,
        viewer.as_ref().map(|user| user.id.as_str()),
        &hidden_author_ids,
    ) {
        Ok(post) => Json(PostResponse::from(post)).into_response(),
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
    let target = match state.forum.post_permission_target(&post_id) {
        Ok(target) => target,
        Err(err) => return forum_error_response(err),
    };
    if !has_capability(
        &state,
        &user,
        "post.edit.own",
        Some(&target.owner_id),
        Some(&target.category_id),
    ) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.forum.edit_post_authorized(&post_id, &payload.content) {
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
    let target = match state.forum.post_permission_target(&post_id) {
        Ok(target) => target,
        Err(err) => return forum_error_response(err),
    };
    let required = if target.is_first_post {
        "topic.delete.own"
    } else {
        "post.delete.own"
    };
    if !has_capability(
        &state,
        &user,
        required,
        Some(&target.owner_id),
        Some(&target.category_id),
    ) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.forum.delete_post_authorized(&post_id) {
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
    let target = match state.forum.topic_permission_target(&topic_id) {
        Ok(target) => target,
        Err(err) => return forum_error_response(err),
    };
    if !has_capability(
        &state,
        &user,
        "topic.delete.own",
        Some(&target.owner_id),
        Some(&target.category_id),
    ) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.forum.delete_topic_authorized(&topic_id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => forum_error_response(err),
    }
}

async fn report_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(post_id): axum::extract::Path<String>,
    Json(payload): Json<ReportPostRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    let can_read_private = can_read_private_categories_for(&state, Some(&user));
    let hidden_author_ids = hidden_author_ids(&state);
    if let Err(err) = state.forum.get_post_visible(
        &post_id,
        can_read_private,
        Some(&user.id),
        &hidden_author_ids,
    ) {
        return forum_error_response(err);
    }

    match state.moderation.report(&user.id, &post_id, &payload.reason) {
        Ok(report) => (StatusCode::CREATED, Json(ReportResponse::from(report))).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn list_reports(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.queue.read", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.open_reports() {
        Ok(reports) => Json(ReportsResponse {
            reports: reports.into_iter().map(ReportResponse::from).collect(),
        })
        .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn resolve_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(report_id): axum::extract::Path<String>,
    Json(payload): Json<ResolveQueueItemRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.queue.write", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state
        .moderation
        .resolve_report(&user.id, &report_id, &payload.resolution)
    {
        Ok(report) => Json(ReportResponse::from(report)).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn list_approvals(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.queue.read", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.open_approvals() {
        Ok(approvals) => Json(ApprovalQueueResponse {
            approvals: approvals
                .into_iter()
                .map(ApprovalItemResponse::from)
                .collect(),
        })
        .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn resolve_approval(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(approval_id): axum::extract::Path<String>,
    Json(payload): Json<ResolveQueueItemRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.queue.write", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state
        .moderation
        .resolve_approval(&user.id, &approval_id, &payload.resolution)
    {
        Ok(approval) => Json(ApprovalItemResponse::from(approval)).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn issue_warning(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<IssueWarningRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "user.warn", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.issue_warning(
        &user.id,
        &payload.target_user_id,
        payload.target_id.as_deref(),
        &payload.reason,
        payload.points,
    ) {
        Ok(warning) => match state.moderation.user_state(&payload.target_user_id) {
            Ok(user_state) => (
                StatusCode::CREATED,
                Json(WarningIssuedResponse {
                    warning: WarningResponse::from(warning),
                    user_state: UserModerationStateResponse::from(user_state),
                }),
            )
                .into_response(),
            Err(err) => moderation_error_response(err),
        },
        Err(err) => moderation_error_response(err),
    }
}

async fn expire_warning(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(warning_id): axum::extract::Path<String>,
    Json(payload): Json<ResolveQueueItemRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "user.warn", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state
        .moderation
        .expire_warning(&user.id, &warning_id, &payload.resolution)
    {
        Ok((warning, user_state)) => Json(WarningIssuedResponse {
            warning: WarningResponse::from(warning),
            user_state: UserModerationStateResponse::from(user_state),
        })
        .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn set_shadowban(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(user_id): axum::extract::Path<String>,
    Json(payload): Json<SetShadowbanRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "user.shadowban", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state
        .moderation
        .set_shadowbanned(&user.id, &user_id, payload.shadowbanned)
    {
        Ok(user_state) => Json(UserModerationStateResponse::from(user_state)).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn execute_macro(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ExecuteModerationMacroRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.macro.execute", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }
    let actions = payload
        .actions
        .into_iter()
        .map(ModerationMacroAction::from)
        .collect::<Vec<_>>();

    match state.moderation.execute_macro(&user.id, &actions) {
        Ok(execution) => (
            StatusCode::CREATED,
            Json(MacroExecutionResponse::from(execution)),
        )
            .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn create_stored_macro(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateModerationMacroRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.macro.write", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }
    let actions = payload
        .actions
        .into_iter()
        .map(ModerationMacroAction::from)
        .collect::<Vec<_>>();

    match state.moderation.create_macro(
        &user.id,
        &payload.name,
        payload.description.as_deref(),
        actions,
    ) {
        Ok(stored) => (
            StatusCode::CREATED,
            Json(StoredModerationMacroResponse::from(stored)),
        )
            .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn list_stored_macros(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.macro.read", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.macros() {
        Ok(macros) => Json(StoredModerationMacrosResponse {
            macros: macros
                .into_iter()
                .map(StoredModerationMacroResponse::from)
                .collect(),
        })
        .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn get_stored_macro(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(macro_id): axum::extract::Path<String>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.macro.read", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.macro_by_id(&macro_id) {
        Ok(stored) => Json(StoredModerationMacroResponse::from(stored)).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn execute_stored_macro(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(macro_id): axum::extract::Path<String>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.macro.execute", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.execute_stored_macro(&user.id, &macro_id) {
        Ok(execution) => (
            StatusCode::CREATED,
            Json(MacroExecutionResponse::from(execution)),
        )
            .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn schedule_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ScheduleModerationJobRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.job.write", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }
    let actions = payload
        .actions
        .into_iter()
        .map(ModerationMacroAction::from)
        .collect::<Vec<_>>();

    match state
        .moderation
        .schedule_job(&user.id, payload.run_at_tick, actions)
    {
        Ok(job) => (StatusCode::CREATED, Json(ModerationJobResponse::from(job))).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn run_due_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<RunDueModerationJobsRequest>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.job.write", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.run_due_jobs(payload.now_tick) {
        Ok(summary) => Json(JobRunSummaryResponse::from(summary)).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn get_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(job_id): axum::extract::Path<String>,
) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "moderation.job.read", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.job(&job_id) {
        Ok(job) => Json(ModerationJobResponse::from(job)).into_response(),
        Err(err) => moderation_error_response(err),
    }
}

async fn list_audit_events(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(user) = authenticated_user(&state, &headers) else {
        return auth_error_response(AuthError::InvalidSession);
    };
    if !has_capability(&state, &user, "audit.read", None, None) {
        return forum_error_response(ForumError::Forbidden);
    }

    match state.moderation.audit_events() {
        Ok(events) => Json(AuditEventsResponse {
            events: events.into_iter().map(AuditEventResponse::from).collect(),
        })
        .into_response(),
        Err(err) => moderation_error_response(err),
    }
}

fn authenticated_user(state: &AppState, headers: &HeaderMap) -> Option<PublicUser> {
    let session_secret = session_secret_from_headers(headers)?;
    state.auth.authenticate(&session_secret).ok()
}

fn can_read_private_categories_for(state: &AppState, user: Option<&PublicUser>) -> bool {
    user.is_some_and(|user| has_capability(state, user, "category.read.private", None, None))
}

fn hidden_author_ids(state: &AppState) -> HashSet<String> {
    state
        .moderation
        .shadowbanned_user_ids()
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn has_capability(
    state: &AppState,
    user: &PublicUser,
    capability: &'static str,
    owner_id: Option<&str>,
    category_id: Option<&str>,
) -> bool {
    let actor = actor_permissions(state, user);
    actor.allows(
        &Capability::parse_static(capability),
        &PermissionContext {
            actor_id: user.id.clone(),
            owner_id: owner_id.map(ToOwned::to_owned),
            category_id: category_id.map(ToOwned::to_owned),
        },
    )
}

fn actor_permissions(state: &AppState, user: &PublicUser) -> ActorPermissions {
    state
        .permissions
        .actor_permissions(&user.id, TrustLevel::from_u8(user.trust_level))
        .unwrap_or_else(|_| ActorPermissions {
            actor_id: user.id.clone(),
            trust_level: TrustLevel::from_u8(user.trust_level),
            global_roles: vec![],
            category_roles: vec![],
        })
}

fn preview_permission_service() -> PermissionService {
    PermissionService::new_in_memory([Role::new(
        "role:preview-member",
        "Preview Member",
        [
            Capability::parse_static("category.create"),
            Capability::parse_static("category.read.private"),
            Capability::parse_static("topic.create"),
            Capability::parse_static("topic.delete.own"),
            Capability::parse_static("post.reply"),
            Capability::parse_static("post.edit.own"),
            Capability::parse_static("post.delete.own"),
        ],
    )])
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

fn moderation_error_response(err: ModerationError) -> Response {
    let status = match &err {
        ModerationError::InvalidReason
        | ModerationError::InvalidPoints
        | ModerationError::EmptyMacro
        | ModerationError::InvalidMacroName => StatusCode::BAD_REQUEST,
        ModerationError::ReportNotFound
        | ModerationError::ApprovalNotFound
        | ModerationError::WarningNotFound
        | ModerationError::MacroNotFound
        | ModerationError::JobNotFound => StatusCode::NOT_FOUND,
        ModerationError::AlreadyResolved | ModerationError::DuplicateMacroName => {
            StatusCode::CONFLICT
        }
        ModerationError::WarningInactive => StatusCode::CONFLICT,
        ModerationError::StorePoisoned => StatusCode::INTERNAL_SERVER_ERROR,
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

impl From<CategoryNode> for CategoryNodeResponse {
    fn from(node: CategoryNode) -> Self {
        Self {
            category: CategoryResponse::from(node.category),
            children: node
                .children
                .into_iter()
                .map(CategoryNodeResponse::from)
                .collect(),
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

impl From<Report> for ReportResponse {
    fn from(report: Report) -> Self {
        Self {
            id: report.id,
            reporter_id: report.reporter_id,
            target_id: report.target_id,
            reason: report.reason,
            status: queue_status_label(report.status),
        }
    }
}

impl From<ApprovalItem> for ApprovalItemResponse {
    fn from(item: ApprovalItem) -> Self {
        Self {
            id: item.id,
            author_id: item.author_id,
            target_id: item.target_id,
            reason: item.reason,
            status: queue_status_label(item.status),
        }
    }
}

impl From<Warning> for WarningResponse {
    fn from(warning: Warning) -> Self {
        Self {
            id: warning.id,
            actor_id: warning.actor_id,
            target_user_id: warning.target_user_id,
            target_id: warning.target_id,
            reason: warning.reason,
            points: warning.points,
            active: warning.active,
        }
    }
}

impl From<UserModerationState> for UserModerationStateResponse {
    fn from(state: UserModerationState) -> Self {
        Self {
            active_warning_points: state.active_warning_points,
            muted: state.muted,
            banned: state.banned,
            shadowbanned: state.shadowbanned,
        }
    }
}

impl From<AuditEvent> for AuditEventResponse {
    fn from(event: AuditEvent) -> Self {
        Self {
            id: event.id,
            actor_id: event.actor_id,
            action: audit_action_label(event.action),
            target_id: event.target_id,
            previous_state: event.previous_state.map(UserModerationStateResponse::from),
            new_state: event.new_state.map(UserModerationStateResponse::from),
            detail: event.detail,
        }
    }
}

impl From<MacroExecution> for MacroExecutionResponse {
    fn from(execution: MacroExecution) -> Self {
        Self {
            action_count: execution.action_count,
            audit_event_count: execution.audit_event_count,
        }
    }
}

impl From<StoredModerationMacro> for StoredModerationMacroResponse {
    fn from(stored: StoredModerationMacro) -> Self {
        Self {
            id: stored.id,
            name: stored.name,
            description: stored.description,
            created_by: stored.created_by,
            actions: stored
                .actions
                .into_iter()
                .map(ModerationMacroActionResponse::from)
                .collect(),
        }
    }
}

impl From<ModerationJob> for ModerationJobResponse {
    fn from(job: ModerationJob) -> Self {
        Self {
            id: job.id,
            actor_id: job.actor_id,
            run_at_tick: job.run_at_tick,
            status: job_status_label(job.status),
            actions: job
                .actions
                .into_iter()
                .map(ModerationMacroActionResponse::from)
                .collect(),
            last_error: job.last_error,
        }
    }
}

impl From<ModerationMacroAction> for ModerationMacroActionResponse {
    fn from(action: ModerationMacroAction) -> Self {
        match action {
            ModerationMacroAction::ResolveReport {
                report_id,
                resolution,
            } => Self::ResolveReport {
                report_id,
                resolution,
            },
            ModerationMacroAction::ResolveApproval {
                approval_id,
                resolution,
            } => Self::ResolveApproval {
                approval_id,
                resolution,
            },
            ModerationMacroAction::IssueWarning {
                target_user_id,
                target_id,
                reason,
                points,
            } => Self::IssueWarning {
                target_user_id,
                target_id,
                reason,
                points,
            },
            ModerationMacroAction::ExpireWarning { warning_id, reason } => {
                Self::ExpireWarning { warning_id, reason }
            }
            ModerationMacroAction::SetShadowban {
                target_user_id,
                shadowbanned,
            } => Self::SetShadowban {
                target_user_id,
                shadowbanned,
            },
        }
    }
}

impl From<JobRunSummary> for JobRunSummaryResponse {
    fn from(summary: JobRunSummary) -> Self {
        Self {
            checked: summary.checked,
            completed: summary.completed,
            failed: summary.failed,
        }
    }
}

impl From<ModerationMacroActionRequest> for ModerationMacroAction {
    fn from(action: ModerationMacroActionRequest) -> Self {
        match action {
            ModerationMacroActionRequest::ResolveReport {
                report_id,
                resolution,
            } => Self::ResolveReport {
                report_id,
                resolution,
            },
            ModerationMacroActionRequest::ResolveApproval {
                approval_id,
                resolution,
            } => Self::ResolveApproval {
                approval_id,
                resolution,
            },
            ModerationMacroActionRequest::IssueWarning {
                target_user_id,
                target_id,
                reason,
                points,
            } => Self::IssueWarning {
                target_user_id,
                target_id,
                reason,
                points,
            },
            ModerationMacroActionRequest::ExpireWarning { warning_id, reason } => {
                Self::ExpireWarning { warning_id, reason }
            }
            ModerationMacroActionRequest::SetShadowban {
                target_user_id,
                shadowbanned,
            } => Self::SetShadowban {
                target_user_id,
                shadowbanned,
            },
        }
    }
}

fn job_status_label(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Pending => "pending",
        JobStatus::Completed => "completed",
        JobStatus::Failed => "failed",
    }
}

fn queue_status_label(status: QueueStatus) -> &'static str {
    match status {
        QueueStatus::Open => "open",
        QueueStatus::Resolved => "resolved",
    }
}

fn audit_action_label(action: AuditAction) -> &'static str {
    match action {
        AuditAction::ReportCreated => "report.created",
        AuditAction::ReportResolved => "report.resolved",
        AuditAction::ApprovalQueued => "approval.queued",
        AuditAction::ApprovalResolved => "approval.resolved",
        AuditAction::WarningIssued => "warning.issued",
        AuditAction::WarningExpired => "warning.expired",
        AuditAction::UserShadowbanSet => "user.shadowban.set",
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
                    "password": test_password()
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
                    "password": test_password()
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
            "password": test_password()
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
                    "password": test_password()
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
                    "password": test_password()
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
                        "password": wrong_test_password()
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
                    "password": test_password()
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
                moderation: ModerationService::new_in_memory(),
                permissions: preview_permission_service(),
                secure_cookies: false,
            },
            32,
        )
        .oneshot(json_request(
            "/api/v1/auth/register",
            json!({
                "username": "Member",
                "email": "member@example.test",
                "password": test_password()
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
    async fn forum_write_routes_require_capabilities() {
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation: ModerationService::new_in_memory(),
                permissions: PermissionService::default(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let response = app
            .oneshot(json_request_with_cookie(
                "/api/v1/categories",
                &cookie,
                json!({
                    "name": "No Permission"
                }),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn private_category_reads_require_private_read_capability() {
        let forum = ForumService::new_in_memory();
        forum
            .create_category("Staff", Some("Private"), None, true)
            .unwrap();
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum,
                moderation: ModerationService::new_in_memory(),
                permissions: PermissionService::new_in_memory([Role::new(
                    "role:no-private-read",
                    "No Private Read",
                    [Capability::parse_static("category.create")],
                )]),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/categories")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            body_json(response).await["categories"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
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

        let category_tree = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/categories/tree")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(category_tree.status(), StatusCode::OK);
        let tree = body_json(category_tree).await;
        assert_eq!(tree["categories"].as_array().unwrap().len(), 1);
        assert_eq!(tree["categories"][0]["category"]["slug"], "general-talk");

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

        let get_reply = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/posts/{reply_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_reply.status(), StatusCode::OK);
        assert_eq!(body_json(get_reply).await["revision"], 2);

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

        let anonymous_post = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/posts/{}",
                        topic["posts"][0]["id"].as_str().unwrap()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous_post.status(), StatusCode::NOT_FOUND);

        let authenticated_topic = app
            .clone()
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

        let authenticated_post = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/posts/{}",
                        topic["posts"][0]["id"].as_str().unwrap()
                    ))
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated_post.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn shadowbanned_topic_author_only_sees_own_topic() {
        let forum = ForumService::new_in_memory();
        let category = forum.create_category("General", None, None, false).unwrap();
        let topic = forum
            .create_topic(&category.id, "user:1", "Shadow Topic", "visible to self")
            .unwrap();
        let topic_id = topic.topic.id.clone();
        let moderation = ModerationService::new_in_memory();
        moderation
            .set_shadowbanned("user:mod", "user:1", true)
            .unwrap();
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum,
                moderation,
                permissions: preview_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
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

        let cookie = register_and_login(&app).await;
        let own_topic = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/topics/{topic_id}"))
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(own_topic.status(), StatusCode::OK);
        assert_eq!(body_json(own_topic).await["topic"]["id"], topic_id);
    }

    #[tokio::test]
    async fn users_can_report_visible_posts_but_not_read_staff_queue() {
        let app = app();
        let cookie = register_and_login(&app).await;
        let category = create_category(&app, &cookie).await;
        let topic = create_topic(&app, &cookie, category["id"].as_str().unwrap()).await;
        let post_id = topic["posts"][0]["id"].as_str().unwrap();

        let report = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/posts/{post_id}/reports"),
                &cookie,
                json!({
                    "reason": "contains private information"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::CREATED);
        let report = body_json(report).await;
        assert_eq!(report["target_id"], post_id);
        assert_eq!(report["status"], "open");
        let report_id = report["id"].as_str().unwrap();

        let resolve_report = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/moderation/reports/{report_id}/resolve"),
                &cookie,
                json!({
                    "resolution": "handled"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resolve_report.status(), StatusCode::FORBIDDEN);

        let reports = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/reports")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reports.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn staff_can_read_moderation_queues_and_audit() {
        let moderation = ModerationService::new_in_memory();
        moderation
            .report("user:reporter", "post:1", "spam")
            .unwrap();
        moderation
            .queue_approval("system:filters", "user:author", "post:2", "low trust link")
            .unwrap();
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation,
                permissions: staff_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let reports = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/reports")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reports.status(), StatusCode::OK);
        assert_eq!(
            body_json(reports).await["reports"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let report_id = body_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/moderation/reports")
                        .header(COOKIE, cookie.as_str())
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await["reports"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let approvals = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/approvals")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(approvals.status(), StatusCode::OK);
        assert_eq!(
            body_json(approvals).await["approvals"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let approval_id = body_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/moderation/approvals")
                        .header(COOKIE, cookie.as_str())
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await["approvals"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();

        let resolve_report = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/moderation/reports/{report_id}/resolve"),
                &cookie,
                json!({
                    "resolution": "deleted duplicate"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resolve_report.status(), StatusCode::OK);
        assert_eq!(body_json(resolve_report).await["status"], "resolved");

        let resolve_approval = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/moderation/approvals/{approval_id}/resolve"),
                &cookie,
                json!({
                    "resolution": "approved"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resolve_approval.status(), StatusCode::OK);
        assert_eq!(body_json(resolve_approval).await["status"], "resolved");

        let open_reports = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/reports")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(open_reports.status(), StatusCode::OK);
        assert!(
            body_json(open_reports).await["reports"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let audit = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/audit")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(audit.status(), StatusCode::OK);
        assert_eq!(
            body_json(audit).await["events"].as_array().unwrap().len(),
            4
        );
    }

    #[tokio::test]
    async fn staff_warning_and_shadowban_routes_update_user_state_and_audit() {
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation: ModerationService::new_in_memory(),
                permissions: staff_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let warning = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/warnings",
                &cookie,
                json!({
                    "target_user_id": "user:target",
                    "target_id": "post:1",
                    "reason": "repeated spam",
                    "points": 10
                }),
            ))
            .await
            .unwrap();
        assert_eq!(warning.status(), StatusCode::CREATED);
        let warning = body_json(warning).await;
        assert_eq!(warning["warning"]["points"], 10);
        assert_eq!(warning["user_state"]["muted"], true);
        assert_eq!(warning["user_state"]["banned"], true);

        let shadowban = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/users/user:target/shadowban",
                &cookie,
                json!({
                    "shadowbanned": true
                }),
            ))
            .await
            .unwrap();
        assert_eq!(shadowban.status(), StatusCode::OK);
        assert_eq!(body_json(shadowban).await["shadowbanned"], true);

        let audit = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/audit")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(audit.status(), StatusCode::OK);
        let audit = body_json(audit).await;
        assert_eq!(audit["events"].as_array().unwrap().len(), 2);
        assert_eq!(audit["events"][0]["action"], "warning.issued");
        assert_eq!(audit["events"][0]["new_state"]["active_warning_points"], 10);
        assert_eq!(audit["events"][1]["action"], "user.shadowban.set");
        assert_eq!(audit["events"][1]["previous_state"]["banned"], true);
    }

    #[tokio::test]
    async fn staff_can_expire_warning_and_recompute_user_state() {
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation: ModerationService::new_in_memory(),
                permissions: staff_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let warning = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/warnings",
                &cookie,
                json!({
                    "target_user_id": "user:target",
                    "reason": "temporary timeout",
                    "points": 10
                }),
            ))
            .await
            .unwrap();
        assert_eq!(warning.status(), StatusCode::CREATED);
        let warning = body_json(warning).await;
        let warning_id = warning["warning"]["id"].as_str().unwrap();
        assert_eq!(warning["user_state"]["banned"], true);

        let expired = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/moderation/warnings/{warning_id}/expire"),
                &cookie,
                json!({
                    "resolution": "points decayed"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(expired.status(), StatusCode::OK);
        let expired = body_json(expired).await;
        assert_eq!(expired["warning"]["active"], false);
        assert_eq!(expired["user_state"]["active_warning_points"], 0);
        assert_eq!(expired["user_state"]["muted"], false);
        assert_eq!(expired["user_state"]["banned"], false);

        let second_expire = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/moderation/warnings/{warning_id}/expire"),
                &cookie,
                json!({
                    "resolution": "again"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(second_expire.status(), StatusCode::CONFLICT);

        let audit = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/audit")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(audit.status(), StatusCode::OK);
        let audit = body_json(audit).await;
        assert_eq!(audit["events"][1]["action"], "warning.expired");
        assert_eq!(audit["events"][1]["previous_state"]["banned"], true);
        assert_eq!(audit["events"][1]["new_state"]["banned"], false);
    }

    #[tokio::test]
    async fn staff_can_execute_transactional_moderation_macro() {
        let moderation = ModerationService::new_in_memory();
        moderation
            .report("user:reporter", "post:1", "spam")
            .unwrap();
        moderation
            .queue_approval("system:filters", "user:target", "post:2", "low trust link")
            .unwrap();
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation,
                permissions: staff_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let macro_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/macros/execute",
                &cookie,
                json!({
                    "actions": [
                        {
                            "type": "resolve_report",
                            "report_id": "report:1",
                            "resolution": "deleted duplicate"
                        },
                        {
                            "type": "resolve_approval",
                            "approval_id": "approval:1",
                            "resolution": "approved"
                        },
                        {
                            "type": "issue_warning",
                            "target_user_id": "user:target",
                            "target_id": "post:2",
                            "reason": "posted spam",
                            "points": 5
                        },
                        {
                            "type": "set_shadowban",
                            "target_user_id": "user:target",
                            "shadowbanned": true
                        }
                    ]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(macro_response.status(), StatusCode::CREATED);
        let macro_response = body_json(macro_response).await;
        assert_eq!(macro_response["action_count"], 4);
        assert_eq!(macro_response["audit_event_count"], 4);

        let open_reports = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/reports")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(open_reports.status(), StatusCode::OK);
        assert!(
            body_json(open_reports).await["reports"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let audit = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/audit")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(audit.status(), StatusCode::OK);
        let audit = body_json(audit).await;
        assert_eq!(audit["events"].as_array().unwrap().len(), 6);
        assert_eq!(audit["events"][2]["action"], "report.resolved");
        assert_eq!(audit["events"][5]["action"], "user.shadowban.set");
    }

    #[tokio::test]
    async fn moderation_macro_api_rolls_back_on_failure() {
        let moderation = ModerationService::new_in_memory();
        moderation
            .report("user:reporter", "post:1", "spam")
            .unwrap();
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation,
                permissions: staff_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let macro_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/macros/execute",
                &cookie,
                json!({
                    "actions": [
                        {
                            "type": "resolve_report",
                            "report_id": "report:1",
                            "resolution": "would resolve"
                        },
                        {
                            "type": "issue_warning",
                            "target_user_id": "user:target",
                            "target_id": null,
                            "reason": "invalid",
                            "points": 0
                        }
                    ]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(macro_response.status(), StatusCode::BAD_REQUEST);

        let open_reports = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/reports")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            body_json(open_reports).await["reports"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let audit = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/audit")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            body_json(audit).await["events"].as_array().unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn moderation_macro_api_requires_capability() {
        let app = app();
        let cookie = register_and_login(&app).await;

        let macro_response = app
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/macros/execute",
                &cookie,
                json!({
                    "actions": []
                }),
            ))
            .await
            .unwrap();

        assert_eq!(macro_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn staff_can_create_list_read_and_execute_stored_macro() {
        let moderation = ModerationService::new_in_memory();
        moderation
            .report("user:reporter", "post:1", "spam")
            .unwrap();
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation,
                permissions: staff_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let create_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/macros",
                &cookie,
                json!({
                    "name": "Spam Cleanup",
                    "description": "Resolve report and warn author",
                    "actions": [
                        {
                            "type": "resolve_report",
                            "report_id": "report:1",
                            "resolution": "removed spam"
                        },
                        {
                            "type": "issue_warning",
                            "target_user_id": "user:target",
                            "target_id": "post:1",
                            "reason": "posted spam",
                            "points": 5
                        }
                    ]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::CREATED);
        let created = body_json(create_response).await;
        assert_eq!(created["id"], "macro:1");
        assert_eq!(created["name"], "Spam Cleanup");
        assert_eq!(created["actions"][0]["type"], "resolve_report");

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/macros")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);
        let listed = body_json(list_response).await;
        assert_eq!(listed["macros"].as_array().unwrap().len(), 1);
        assert_eq!(listed["macros"][0]["created_by"], "user:1");

        let get_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/macros/macro:1")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_response.status(), StatusCode::OK);
        assert_eq!(
            body_json(get_response).await["description"],
            "Resolve report and warn author"
        );

        let execute_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/macros/macro:1/execute",
                &cookie,
                json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(execute_response.status(), StatusCode::CREATED);
        let execution = body_json(execute_response).await;
        assert_eq!(execution["action_count"], 2);
        assert_eq!(execution["audit_event_count"], 2);

        let reports_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/reports")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            body_json(reports_response).await["reports"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn stored_macro_api_requires_capabilities() {
        let app = app();
        let cookie = register_and_login(&app).await;

        let create_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/macros",
                &cookie,
                json!({
                    "name": "Spam Cleanup",
                    "description": null,
                    "actions": []
                }),
            ))
            .await
            .unwrap();
        assert_eq!(create_response.status(), StatusCode::FORBIDDEN);

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/macros")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list_response.status(), StatusCode::FORBIDDEN);

        let execute_response = app
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/macros/macro:1/execute",
                &cookie,
                json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(execute_response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn staff_can_schedule_run_and_read_moderation_job() {
        let moderation = ModerationService::new_in_memory();
        moderation
            .report("user:reporter", "post:1", "spam")
            .unwrap();
        let app = app_with_state(
            AppState {
                auth: AuthService::new_in_memory(),
                forum: ForumService::new_in_memory(),
                moderation,
                permissions: staff_permission_service(),
                secure_cookies: true,
            },
            1_048_576,
        );
        let cookie = register_and_login(&app).await;

        let schedule_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/jobs",
                &cookie,
                json!({
                    "run_at_tick": 5,
                    "actions": [
                        {
                            "type": "resolve_report",
                            "report_id": "report:1",
                            "resolution": "handled by delayed job"
                        }
                    ]
                }),
            ))
            .await
            .unwrap();
        assert_eq!(schedule_response.status(), StatusCode::CREATED);
        let scheduled = body_json(schedule_response).await;
        assert_eq!(scheduled["id"], "job:1");
        assert_eq!(scheduled["status"], "pending");
        assert_eq!(scheduled["actions"][0]["type"], "resolve_report");

        let early_run = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/jobs/run-due",
                &cookie,
                json!({
                    "now_tick": 4
                }),
            ))
            .await
            .unwrap();
        assert_eq!(early_run.status(), StatusCode::OK);
        let early_run = body_json(early_run).await;
        assert_eq!(early_run["checked"], 0);
        assert_eq!(early_run["completed"], 0);

        let due_run = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/jobs/run-due",
                &cookie,
                json!({
                    "now_tick": 5
                }),
            ))
            .await
            .unwrap();
        assert_eq!(due_run.status(), StatusCode::OK);
        let due_run = body_json(due_run).await;
        assert_eq!(due_run["checked"], 1);
        assert_eq!(due_run["completed"], 1);
        assert_eq!(due_run["failed"], 0);

        let job_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/jobs/job:1")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(job_response.status(), StatusCode::OK);
        let job_response = body_json(job_response).await;
        assert_eq!(job_response["status"], "completed");
        assert_eq!(job_response["last_error"], serde_json::Value::Null);

        let open_reports = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/reports")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(open_reports.status(), StatusCode::OK);
        assert!(
            body_json(open_reports).await["reports"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn moderation_job_api_requires_capability() {
        let app = app();
        let cookie = register_and_login(&app).await;

        let schedule_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/jobs",
                &cookie,
                json!({
                    "run_at_tick": 1,
                    "actions": []
                }),
            ))
            .await
            .unwrap();
        assert_eq!(schedule_response.status(), StatusCode::FORBIDDEN);

        let run_response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/moderation/jobs/run-due",
                &cookie,
                json!({
                    "now_tick": 1
                }),
            ))
            .await
            .unwrap();
        assert_eq!(run_response.status(), StatusCode::FORBIDDEN);

        let read_response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/moderation/jobs/job:1")
                    .header(COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(read_response.status(), StatusCode::FORBIDDEN);
    }

    async fn register_and_login(app: &Router) -> String {
        let register_response = app
            .clone()
            .oneshot(json_request(
                "/api/v1/auth/register",
                json!({
                    "username": "ForumMember",
                    "email": "forum-member@example.test",
                    "password": test_password()
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
                    "password": test_password()
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

    async fn create_category(app: &Router, cookie: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(json_request_with_cookie(
                "/api/v1/categories",
                cookie,
                json!({
                    "name": "General"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        body_json(response).await
    }

    async fn create_topic(app: &Router, cookie: &str, category_id: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(json_request_with_cookie(
                &format!("/api/v1/categories/{category_id}/topics"),
                cookie,
                json!({
                    "title": "Reportable Topic",
                    "content": "visible content"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        body_json(response).await
    }

    fn staff_permission_service() -> PermissionService {
        PermissionService::new_in_memory([Role::new(
            "role:preview-staff",
            "Preview Staff",
            [
                Capability::parse_static("category.create"),
                Capability::parse_static("category.read.private"),
                Capability::parse_static("topic.create"),
                Capability::parse_static("topic.delete.own"),
                Capability::parse_static("post.reply"),
                Capability::parse_static("post.edit.own"),
                Capability::parse_static("post.delete.own"),
                Capability::parse_static("moderation.queue.read"),
                Capability::parse_static("moderation.queue.write"),
                Capability::parse_static("moderation.macro.read"),
                Capability::parse_static("moderation.macro.write"),
                Capability::parse_static("moderation.macro.execute"),
                Capability::parse_static("moderation.job.read"),
                Capability::parse_static("moderation.job.write"),
                Capability::parse_static("user.warn"),
                Capability::parse_static("user.shadowban"),
                Capability::parse_static("audit.read"),
            ],
        )])
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

    fn test_password() -> String {
        "a".repeat(32)
    }

    fn wrong_test_password() -> String {
        "b".repeat(32)
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
