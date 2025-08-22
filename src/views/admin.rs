// src/views/admin.rs

use axum::response::{Html, IntoResponse};

pub fn admin_page() -> impl IntoResponse {
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