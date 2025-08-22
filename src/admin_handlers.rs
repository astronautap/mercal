// src/admin_handlers.rs

use crate::auth::{self, AppState, User};
use crate::users;
use crate::escala::Genero;
use axum::{
    debug_handler,
    extract::{Form, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use tower_cookies::Cookies;
// ADICIONADO: Importar o novo módulo de views
use crate::views;

#[derive(Debug, Deserialize)]
pub struct ChangePasswordForm {
    username: String,
    new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserForm {
    username: String,
    name: String,
    new_password: String,
    ano: u8,
    curso: char,
    genero: Genero,
    turma: String,
}

/// Apresenta a página de administração.
#[debug_handler]
pub async fn admin_page_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    // --- CORRIGIDO: Adicionado .await ---
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, "Acesso negado. Apenas para administradores.").into_response();
    }

    // MODIFICADO: Chama a função da view em vez de ter o HTML aqui
    views::admin::admin_page().into_response()
}

/// Processa a alteração de senha.
#[debug_handler]
pub async fn change_password_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<ChangePasswordForm>,
) -> impl IntoResponse {
    // --- CORRIGIDO: Adicionado .await ---
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }

    let users_to_save;
    {
        let mut users_map = state.users.lock().unwrap();
        if let Some(user) = users_map.get_mut(&form.username) {
            user.password = match bcrypt::hash(&form.new_password, bcrypt::DEFAULT_COST) {
                Ok(h) => h,
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao gerar hash da senha.").into_response(),
            };
        } else {
            return (StatusCode::NOT_FOUND, format!("Utilizador '{}' não encontrado.", form.username)).into_response();
        }
        users_to_save = users_map.clone();
    }

    if let Err(e) = users::save_users(&users_to_save).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Erro ao guardar ficheiro: {}", e)).into_response();
    }
    println!("✅ Senha do utilizador '{}' alterada com sucesso.", form.username);
    Redirect::to("/admin").into_response()
}

/// Processa a criação de utilizador.
#[debug_handler]
pub async fn create_user_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<CreateUserForm>,
) -> impl IntoResponse {
    // --- CORRIGIDO: Adicionado .await ---
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }

    let users_to_save;
    {
        let mut users_map = state.users.lock().unwrap();
        if users_map.contains_key(&form.username) {
            return (StatusCode::CONFLICT, format!("O utilizador '{}' já existe.", form.username)).into_response();
        }
        let hashed_password = match bcrypt::hash(&form.new_password, bcrypt::DEFAULT_COST) {
            Ok(h) => h,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao gerar hash da senha.").into_response(),
        };

        let roles = if form.username.contains("admin") {
            vec!["admin".to_string()]
        } else if form.username.contains("rancheiro") {
            vec!["rancheiro".to_string()]
        } else {
            vec![]
        };

        let new_user = User {
            id: form.username.clone(),
            password: hashed_password,
            name: form.name,
            turma: form.turma,
            ano: form.ano,
            curso: form.curso,
            genero: form.genero,
            roles,
        };
        users_map.insert(form.username.clone(), new_user);
        users_to_save = users_map.clone();
    }

    if let Err(e) = users::save_users(&users_to_save).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Erro ao guardar o ficheiro de utilizadores: {}", e)).into_response();
    }
    println!("✅ Utilizador '{}' criado com sucesso.", form.username);
    Redirect::to("/admin").into_response()
}