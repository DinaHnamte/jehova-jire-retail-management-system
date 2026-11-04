use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header::SET_COOKIE},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

use super::{
    dto::{LoginRequest, SessionResponse, SessionUser, UserListItem},
    password, repo,
    session::{self, CurrentUser},
};
use crate::{
    AppState, audit,
    error::{AppError, AppResult},
    http::{auth::ClientMeta, json::ApiJson},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/users", get(list_users))
}

const MAX_USERNAME: usize = 64;
const MAX_PASSWORD: usize = 1024;

async fn login(
    State(state): State<AppState>,
    meta: ClientMeta,
    ApiJson(req): ApiJson<LoginRequest>,
) -> AppResult<Response> {
    let username = req.username.trim().to_string();
    if username.is_empty() || req.password.is_empty() {
        return Err(AppError::Validation(
            "Enter a username and password.".into(),
        ));
    }
    if username.len() > MAX_USERNAME || req.password.len() > MAX_PASSWORD {
        return Err(AppError::Validation(
            "Username or password is too long.".into(),
        ));
    }
    let ip = meta.ip.as_deref();
    if !state.login_limiter.try_acquire(&username, ip) {
        return Err(AppError::RateLimited);
    }

    // Release the connection before the (slow) password check, so concurrent logins
    // can't hold the whole pool.
    let user = {
        let mut conn = state.db.acquire().await?;
        repo::find_user_for_login(&mut conn, &username).await?
    };
    let hash = user
        .as_ref()
        .map(|u| u.password_hash.clone())
        .unwrap_or_else(|| password::dummy_hash().to_string());
    let pw = req.password;
    let verified = tokio::task::spawn_blocking(move || password::verify_password(&pw, &hash))
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let user = match user {
        Some(u) if verified && u.is_active => u,
        _ => {
            // The reserved attempt stays counted as a failure.
            let mut conn = state.db.acquire().await?;
            audit::record(
                &mut conn,
                audit::Entry {
                    actor_id: None,
                    action: "auth.login_failed",
                    entity_type: "user",
                    entity_id: None,
                    before: None,
                    after: Some(json!({ "username": username, "ip": ip })),
                },
            )
            .await?;
            return Err(AppError::InvalidCredentials);
        }
    };
    state.login_limiter.succeeded(&username, ip);

    let mut tx = state.db.begin().await?;
    let roles = repo::user_role_codes(&mut tx, user.id).await?;
    let new = session::create(
        &mut tx,
        user.id,
        session::idle_secs_for_roles(&roles),
        meta.user_agent.as_deref(),
        ip,
    )
    .await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(user.id),
            action: "auth.login",
            entity_type: "user",
            entity_id: Some(user.id),
            before: None,
            after: Some(json!({ "ip": ip })),
        },
    )
    .await?;
    let current = session::lookup(&mut tx, &new.token)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("new session not found")))?;
    tx.commit().await?;

    Ok((
        [(
            SET_COOKIE,
            session::set_cookie(&new.token, state.config.cookie_secure),
        )],
        Json(session_response(current)),
    )
        .into_response())
}

async fn logout(State(state): State<AppState>, user: CurrentUser) -> AppResult<Response> {
    let mut tx = state.db.begin().await?;
    session::delete(&mut tx, user.session_id).await?;
    audit::record(
        &mut tx,
        audit::Entry {
            actor_id: Some(user.id),
            action: "auth.logout",
            entity_type: "user",
            entity_id: Some(user.id),
            before: None,
            after: None,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(
            SET_COOKIE,
            session::clear_cookie(state.config.cookie_secure),
        )],
    )
        .into_response())
}

async fn me(user: CurrentUser) -> Json<SessionResponse> {
    Json(session_response(user))
}

async fn list_users(
    State(state): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<Vec<UserListItem>>> {
    user.require("users.manage")?;
    let rows = sqlx::query_as!(
        UserListItem,
        r#"SELECT u.id, u.username, u.display_name, u.is_active,
                  COALESCE(array_agg(r.code ORDER BY r.code) FILTER (WHERE r.code IS NOT NULL), '{}') AS "roles!"
           FROM users u
           LEFT JOIN user_roles ur ON ur.user_id = u.id
           LEFT JOIN roles r ON r.id = ur.role_id
           GROUP BY u.id
           ORDER BY lower(u.username)"#
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

fn session_response(user: CurrentUser) -> SessionResponse {
    let mut permissions: Vec<String> = user.permissions.into_iter().collect();
    permissions.sort();
    SessionResponse {
        user: SessionUser {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            permissions,
        },
        csrf_token: user.csrf_token,
    }
}
