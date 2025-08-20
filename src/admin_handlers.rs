// src/admin_handlers.rs

use crate::auth::{self, AppState, User};
use crate::users;
use crate::escala::Genero;
use axum::{
    debug_handler,
    extract::{Form, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use tower_cookies::Cookies;

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

    Html(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Admin</title>
            <style>
                body { font-family: Arial, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; background: #f5f5f5; }
                .container { background: white; padding: 30px; border-radius: 10px; box-shadow: 0 2px 10px rgba(0,0,0,0.1); margin-bottom: 30px;}
                h1, h2 { text-align: center; color: #343a40; }
                h1 { color: #dc3545; }
                form { display: flex; flex-direction: column; gap: 15px; }
                input { width: 100%; padding: 12px; border: 1px solid #ddd; border-radius: 5px; box-sizing: border-box; }
                button { padding: 12px; color: white; border: none; border-radius: 5px; cursor: pointer; font-size: 16px; }
                .btn-change { background: #dc3545; }
                .btn-change:hover { background: #c82333; }
                .btn-create { background: #007bff; }
                .btn-create:hover { background: #0056b3; }
                .nav-link { display: block; text-align: center; margin-top: 20px; color: #007bff; }
            </style>
        </head>
        <body>
            <div class="container">
                <h1>🔑 Administração de Senhas</h1>
                <form method="POST" action="/admin/change-password">
                    <input type="text" name="username" placeholder="Username do Utilizador a Alterar" required />
                    <input type="password" name="new_password" placeholder="Nova Senha" required />
                    <button type="submit" class="btn-change">Alterar Senha</button>
                </form>
            </div>
            <div class="container">
                <h2>👤 Criar Novo Utilizador</h2>
                <form method="POST" action="/admin/create-user">
                    <input type="text" name="username" placeholder="ID do Novo Utilizador (username)" required />
                    <input type="text" name="name" placeholder="Nome Completo do Novo Utilizador" required />
                    <input type="text" name="turma" placeholder="Turma (ex: T100)" required />
                    <input type="text" name="ano" placeholder="Ano" required />
                    <input type="text" name="curso" placeholder="Curso" required />
                    <input type="text" name="genero" placeholder="Gênero" required />
                    <input type="password" name="new_password" placeholder="Senha do Novo Utilizador" required />
                    <button type="submit" class="btn-create">Criar Utilizador</button>
                </form>
            </div>
            <a href="/dashboard" class="nav-link">← Voltar ao Dashboard</a>
        </body>
        </html>
        "#,
    ).into_response()
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
