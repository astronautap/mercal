// src/handlers.rs

use crate::auth::{self, AppState, LoginForm};
use crate::users;
use axum::http::StatusCode;
use axum::{
    debug_handler,
    extract::{Form, State},
    response::{IntoResponse, Redirect},
};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;
use crate::views::dashboard as view;

// Estrutura para a mensagem e constante do ficheiro
const DASHBOARD_MESSAGE_FILE: &str = "data/dashboard_message.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DashboardMessage {
    pub content: String,
    pub author_name: String,
    pub author_role: String,
    pub timestamp: DateTime<Local>,
}

#[debug_handler]
pub async fn login_page() -> impl IntoResponse {
    view::login_page(None)
}

#[debug_handler]
pub async fn login_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(login): Form<LoginForm>,
) -> impl IntoResponse {
    if let Ok(fresh_users) = users::load_users().await {
        *state.users.lock().unwrap() = fresh_users;
    }

    let users = state.users.lock().unwrap();
    if let Some(user) = users.get(&login.username) {
        if bcrypt::verify(&login.password, &user.password).unwrap_or(false) {
            let session_id = Uuid::new_v4().to_string();
            state.sessions.lock().unwrap().insert(session_id.clone());
            cookies.add(Cookie::new("session_id", session_id));
            cookies.add(Cookie::new("user_id", login.username));
            return Redirect::to("/dashboard").into_response();
        }
    }
    
    view::login_page(Some("Usuário ou senha incorretos.")).into_response()
}

#[debug_handler]
pub async fn dashboard_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let session_id = cookies.get("session_id").map(|c| c.value().to_string());
    let user_id_cookie = cookies.get("user_id");

    if session_id.is_none()
        || user_id_cookie.is_none()
        || !state.sessions.lock().unwrap().contains(&session_id.unwrap())
    {
        return Redirect::to("/").into_response();
    }

    let is_admin = auth::has_role(&state, &cookies, "admin").await;

    let message = match fs::read_to_string(DASHBOARD_MESSAGE_FILE).await {
        Ok(content) => serde_json::from_str::<DashboardMessage>(&content).ok(),
        Err(_) => None,
    };
    
    view::render_dashboard_page(&state, &cookies, is_admin, message).await.into_response()
}

#[debug_handler]
pub async fn logout_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if let Some(cookie) = cookies.get("session_id") {
        state.sessions.lock().unwrap().remove(cookie.value());
        cookies.remove(Cookie::from("session_id"));
        cookies.remove(Cookie::from("user_id"));
    }
    
    view::login_page(None).into_response()
}

#[derive(Debug, Deserialize)]
pub struct DashboardMessageForm {
    content: String,
}

#[debug_handler]
pub async fn update_dashboard_message_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<DashboardMessageForm>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await {
        // CORREÇÃO: Adicionado .into_response()
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }

    let user_id = cookies.get("user_id").unwrap().value().to_string();
    let (author_name, author_role) = {
        let users = state.users.lock().unwrap();
        let user = users.get(&user_id).cloned().unwrap();
        let role = user
            .roles
            .get(0)
            .cloned()
            .unwrap_or_else(|| "Admin".to_string());
        (user.name, role)
    };

    let new_message = DashboardMessage {
        content: form.content,
        author_name,
        author_role,
        timestamp: Local::now(),
    };

    if let Ok(json) = serde_json::to_string_pretty(&new_message) {
        if fs::write(DASHBOARD_MESSAGE_FILE, json).await.is_err() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Falha ao guardar a mensagem.",
            )
                .into_response();
        }
    }

    Redirect::to("/dashboard").into_response()
}