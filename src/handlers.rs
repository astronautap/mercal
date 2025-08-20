// src/handlers.rs

use crate::auth::{self, AppState, LoginForm};
use crate::escala::{self, EscalaDiaria, EstadoEscala, StatusTroca, Troca};
use crate::meals;
use crate::users;
use axum::{
    debug_handler,
    extract::{Form, State},
    response::{IntoResponse, Redirect},
};
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;

// --- MÓDULO DE VISUALIZAÇÃO (HTML e CSS) ---
// Para manter o código organizado, todo o HTML e CSS fica aqui.
mod view {
    use super::{auth, AppState, Cookies}; // Importa o que for necessário para a lógica de visualização
    use axum::response::{Html, IntoResponse};
    use chrono::{Datelike, Weekday};
    use std::collections::{BTreeMap, HashMap};

    // 1. CSS CENTRALIZADO
    // Todo o estilo Material Design em um único lugar. Fácil de editar.
    const CSS: &str = r#"
        :root {
            --primary-color: #3f51b5; /* Indigo */
            --primary-dark: #303f9f;
            --accent-color: #ff4081; /* Pink */
            --background-color: #f5f5f5;
            --card-background: #ffffff;
            --text-color: #212121;
            --text-light: #757575;
            --border-color: #e0e0e0;
            --shadow: 0 2px 4px rgba(0,0,0,0.1), 0 2px 10px rgba(0,0,0,0.08);
            --success-color: #4caf50;
            --danger-color: #f44336;
        }
        body {
            font-family: 'Roboto', -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
            background-color: var(--background-color);
            color: var(--text-color);
            margin: 0;
            line-height: 1.6;
        }
        .container { max-width: 1200px; margin: 20px auto; padding: 0 15px; }
        .card {
            background-color: var(--card-background);
            border-radius: 8px;
            box-shadow: var(--shadow);
            padding: 24px;
            overflow: hidden;
        }
        .card-title {
            font-size: 1.25em;
            font-weight: 500;
            margin: 0 0 16px 0;
            padding-bottom: 16px;
            border-bottom: 1px solid var(--border-color);
            display: flex;
            align-items: center;
        }
        .card-title .icon { font-size: 1.5em; margin-right: 12px; color: var(--primary-color); }
        .btn {
            padding: 10px 24px; border: none; border-radius: 4px; text-decoration: none;
            font-weight: 500; cursor: pointer; transition: all 0.2s ease-in-out;
            display: inline-block; text-align: center; font-size: 14px;
            text-transform: uppercase; letter-spacing: 0.5px;
        }
        .btn-primary { background-color: var(--primary-color); color: white; }
        .btn-primary:hover { background-color: var(--primary-dark); box-shadow: 0 4px 12px rgba(0,0,0,0.2); }
        .btn-accent { background-color: var(--accent-color); color: white; }
        .btn-full { width: 100%; box-sizing: border-box; }
        
        /* --- ESTILOS GERAIS DE INPUT --- */
        input[type="text"], input[type="password"] {
            width: 100%; padding: 14px; margin: 8px 0; border: 1px solid var(--border-color);
            border-radius: 4px; box-sizing: border-box; font-size: 16px;
        }
        
        /* --- ESTILOS DA PÁGINA DE LOGIN --- */
        .login-body {
            background: var(--background-color);
        }
        .login-container { max-width: 400px; margin: 10vh auto; }
        .login-card {
            background: var(--card-background);
            border-radius: 12px;
            box-shadow: 0 10px 25px rgba(0,0,0,0.1);
            padding: 60px;
            text-align: center;
        }
        .login-header {
            margin-bottom: 15px;
        }

        .login-header h1 {
            margin: 0;
            font-size: 1.8em;
            color: var(--text-color);
        }
        .username-input {
            text-align: center;
            font-size: 1.2em;
            letter-spacing: 2px;
        }
        .password-container {
            display: flex;
            justify-content: center;
            gap: 10px;
            margin: 10px 0;
            cursor: text;
            outline: none;
            margin-bottom: 20px;
        }
        .password-box {
            width: 30px;
            height: 40px;
            border: 2px solid var(--border-color);
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 1.5em;
            font-family: monospace;
            transition: border-color 0.2s;
        }
        .password-container:focus .password-box.active,
        .password-box.active {
            border-color: var(--primary-color);
        }
        .info-box { background: #f1f1f1; padding: 10px; border-radius: 6px; margin-top: 25px; font-size: 13px; color: var(--text-light); }
        
        /* --- ESTILOS DO DASHBOARD --- */
        .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem; }
        .item-list { list-style: none; padding: 0; margin: 0; }
        .item-list li { padding: 10px 0; border-bottom: 1px solid #f0f0f0; }
        .item-list li:last-child { border-bottom: none; }
        .features-grid { 
            display: flex; 
            flex-direction: column;
            gap: 12px;
        }
        .features-grid .btn { width: 100%; box-sizing: border-box; }
        .btn-small-success { background-color: var(--success-color); color: white; padding: 6px 12px; font-size: 12px;}
        .btn-small-danger { background-color: var(--danger-color); color: white; padding: 6px 12px; font-size: 12px;}
        .dashboard-grid {
            display: grid;
            grid-template-columns: 2fr 1fr;
            gap: 25px;
            align-items: start;
        }
        .main-column, .sidebar-column {
            display: flex;
            flex-direction: column;
            gap: 25px;
        }
        .info-features-grid {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 25px;
            align-items: start;
        }
        @media (max-width: 1100px) { .info-features-grid { grid-template-columns: 1fr; } }
        @media (max-width: 900px) { .dashboard-grid { grid-template-columns: 1fr; } }
        .trade-item {
            border: 1px solid var(--border-color);
            border-radius: 6px;
            padding: 16px;
            margin-bottom: 12px;
        }
        .trade-item:last-child { margin-bottom: 0; }
        .trade-details { margin-bottom: 12px; font-size: 1em; }
        .trade-details .icon { color: var(--primary-color); margin-right: 8px; }
        .trade-actions { display: flex; gap: 10px; margin-top: 10px; }
        .status-tag {
            padding: 4px 10px; border-radius: 12px; font-size: 12px;
            font-weight: 500; color: white; text-transform: uppercase;
            display: inline-block;
        }
        .status-pending { background-color: #ffc107; }
        .status-approved { background-color: var(--success-color); }
        .status-rejected { background-color: var(--danger-color); }
        .schedule-day {
            display: flex;
            align-items: center;
            gap: 16px;
            padding: 12px 0;
            border-bottom: 1px solid var(--border-color);
        }
        .schedule-day:last-child { border-bottom: none; }
        .date-badge {
            flex-shrink: 0; width: 60px; height: 60px;
            background-color: #e8eaf6;
            border-radius: 8px; display: flex; flex-direction: column;
            align-items: center; justify-content: center; font-weight: 500;
        }
        .date-badge span:first-child { font-size: 1.1em; color: var(--primary-dark); }
        .date-badge span:last-child { font-size: 0.8em; text-transform: uppercase; color: var(--primary-color); }
        .service-info p { margin: 0; }
        .service-info p strong { font-weight: 500; }
    "#;

    // 2. FUNÇÃO DE LAYOUT
    fn render_page(title: &str, content: String, body_class: &str) -> Html<String> {
        Html(format!(
            r#"
            <!DOCTYPE html>
            <html lang="pt-BR">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>{title}</title>
                <link rel="preconnect" href="https://fonts.googleapis.com">
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
                <link href="https://fonts.googleapis.com/css2?family=Roboto:wght@400;500;700&display=swap" rel="stylesheet">
                <style>{CSS}</style>
            </head>
            <body class="{body_class}">
                <div class="container">
                    {content}
                </div>
            </body>
            </html>
            "#,
            title = title,
            CSS = CSS,
            content = content,
            body_class = body_class
        ))
    }

    // --- RENDERIZADORES DE PÁGINAS COMPLETAS ---

    pub fn login_page(error: Option<&str>) -> Html<String> {
        let error_html = if let Some(err) = error {
            format!("<p style='color: var(--danger-color); text-align: center;'>{}</p>", err)
        } else {
            "".to_string()
        };

        let content = format!(
            r#"
            <div class="login-container">
                <div class="login-card">
                    <div class="login-header">
                        <h1>Área Restrita</h1>
                    </div>
                    <form method="POST" action="/login">
                        <input type="text" name="username" placeholder="Número Interno" required maxlength="4" class="username-input" />
                        
                        <input type="password" name="password" id="real-password-input" required style="display:none;">
                        
                        <div class="password-container" id="password-boxes" tabindex="0">
                            <div class="password-box"></div>
                            <div class="password-box"></div>
                            <div class="password-box"></div>
                            <div class="password-box"></div>
                            <div class="password-box"></div>
                        </div>

                        {error_html}
                        <button type="submit" class="btn btn-primary btn-full">Entrar</button>
                    </form>
                    <div class="info-box">
                        <strong>Versão do Sistema:</strong>
                        1.0 - OUT/2025
                    </div>
                </div>
            </div>

            <script>
                const passwordContainer = document.getElementById('password-boxes');
                const passwordBoxes = passwordContainer.querySelectorAll('.password-box');
                const realPasswordInput = document.getElementById('real-password-input');
                let password = '';

                const activateInput = () => {{
                    const tempInput = document.createElement('input');
                    tempInput.type = 'number';
                    tempInput.style.position = 'absolute';
                    tempInput.style.opacity = '0';
                    document.body.appendChild(tempInput);
                    tempInput.focus();

                    tempInput.addEventListener('input', (e) => {{
                        const value = e.target.value;
                        if (value.length <= 5) {{
                            password = value;
                            updatePasswordDisplay();
                        }} else {{
                            e.target.value = password;
                        }}
                    }});

                    tempInput.addEventListener('blur', () => {{
                        document.body.removeChild(tempInput);
                    }});
                }};

                passwordContainer.addEventListener('click', activateInput);
                passwordContainer.addEventListener('focus', activateInput);

                function updatePasswordDisplay() {{
                    realPasswordInput.value = password;
                    passwordBoxes.forEach((box, index) => {{
                        if (index < password.length) {{
                            box.textContent = '●';
                            box.classList.remove('active');
                        }} else {{
                            box.textContent = '';
                            box.classList.remove('active');
                        }}
                    }});
                    if (password.length < 5) {{
                        passwordBoxes[password.length].classList.add('active');
                    }}
                }}
                
                updatePasswordDisplay();
            </script>
            "#
        );
        render_page("Login", content, "login-body")
    }

    // --- RENDERIZADORES DE COMPONENTES DO DASHBOARD ---
    
    fn weekday_to_portuguese(weekday: Weekday) -> &'static str {
        match weekday {
            Weekday::Mon => "Seg", Weekday::Tue => "Ter", Weekday::Wed => "Qua",
            Weekday::Thu => "Qui", Weekday::Fri => "Sex", Weekday::Sat => "Sáb", Weekday::Sun => "Dom",
        }
    }

    pub async fn render_schedule_card(user_id: &str, escala_period: Option<(chrono::NaiveDate, chrono::NaiveDate)>) -> String {
        let Ok(estado_content) = tokio::fs::read_to_string("data/escala/estado.json").await else { return "".to_string(); };
        let Ok(estado) = serde_json::from_str::<super::EstadoEscala>(&estado_content) else { return "".to_string(); };
        
        let mut services_by_date: BTreeMap<chrono::NaiveDate, Vec<String>> = BTreeMap::new();
        let mut current_date = estado.periodo_atual.start_date;

        while current_date <= estado.periodo_atual.end_date {
            let filename = format!("{}/{}.json", super::escala::ESCALA_DATA_DIR, current_date.format("%Y-%m-%d"));
            if let Ok(content) = tokio::fs::read_to_string(filename).await {
                if let Ok(escala_diaria) = serde_json::from_str::<super::EscalaDiaria>(&content) {
                    for (posto, horarios) in &escala_diaria.escala {
                        for (horario, alocacao) in horarios {
                            if alocacao.user_id == user_id {
                                let service_details = format!("<p><strong>{}</strong> às {}</p>", posto, horario);
                                services_by_date.entry(current_date).or_default().push(service_details);
                            }
                        }
                    }
                }
            }
            current_date = current_date.succ_opt().unwrap_or(current_date);
        }
        
        let mut services_html = String::new();
        if services_by_date.is_empty() {
            services_html = "<p>Você não está escalado para nenhum serviço no período ativo.</p>".to_string();
        } else {
            for (date, services) in services_by_date {
                services_html.push_str(&format!(
                    r#"
                    <div class="schedule-day">
                        <div class="date-badge">
                            <span>{dia}</span>
                            <span>{mes}</span>
                        </div>
                        <div class="service-info">
                            {service_list}
                        </div>
                    </div>
                    "#,
                    dia = date.format("%d"),
                    mes = weekday_to_portuguese(date.weekday()),
                    service_list = services.join("")
                ));
            }
        }

        let periodo_html = if let Some((start, end)) = escala_period {
            format!("<div style='color: var(--text-light); font-size: 0.9em; margin-bottom: 10px; border-bottom: 1px solid var(--border-color); padding-bottom: 10px;'>Período: {} a {}</div>", start.format("%d/%m/%Y"), end.format("%d/%m/%Y"))
        } else {
            String::new()
        };

        format!(r#"
            <div class="card">
                <h2 class="card-title"><span class="icon">📅</span> Meus Serviços na Escala</h2>
                {periodo_html}
                <div>{services_html}</div>
            </div>
        "#)
    }

    pub async fn render_meals_card(user_id: &str) -> String {
        let Ok(form_state) = super::meals::load_form_state().await else { return "".to_string() };
        let mut interests_html = String::new();
        let mut current_date = form_state.active_period.start_date;

        while current_date <= form_state.active_period.end_date {
            if let Ok(daily_data) = super::meals::load_daily_meals(current_date).await {
                if let Some(selection) = daily_data.get(user_id) {
                    let daily: Vec<&str> = [
                        (selection.cafe, "Café"), (selection.almoco, "Almoço"),
                        (selection.janta, "Jantar"), (selection.ceia, "Ceia"),
                    ].iter().filter(|(sel, _)| *sel).map(|(_, name)| *name).collect();

                    if !daily.is_empty() {
                        interests_html.push_str(&format!(
                            "<li><strong>{dia} ({data})</strong>: {refeicoes}</li>",
                            dia = weekday_to_portuguese(current_date.weekday()),
                            data = current_date.format("%d/%m"),
                            refeicoes = daily.join(", ")
                        ));
                    }
                }
            }
            current_date = current_date.succ_opt().unwrap_or(current_date);
        }
        
        if interests_html.is_empty() {
            interests_html = "<li>Você não marcou interesse em nenhuma refeição.</li>".to_string();
        }

        format!(r#"
            <div class="card">
                <h2 class="card-title"><span class="icon">🍳</span> Interesses de Refeição</h2>
                <ul class="item-list">{interests_html}</ul>
            </div>
        "#)
    }

    pub async fn render_trades_content(user_id: &str, users_map: &HashMap<String, crate::auth::User>) -> String {
        let Ok(trocas_content) = tokio::fs::read_to_string("data/escala/trocas.json").await else { return "".to_string() };
        let Ok(todas_as_trocas) = serde_json::from_str::<Vec<super::Troca>>(&trocas_content) else { return "".to_string() };
        
        let mut trades_html = String::new();

        for troca in todas_as_trocas.iter().filter(|t| t.requerente.user_id == user_id || t.alvo.user_id == user_id) {
            let requerente_nome = users_map.get(&troca.requerente.user_id).map_or("N/A", |u| u.name.as_str());
            let alvo_nome = users_map.get(&troca.alvo.user_id).map_or("N/A", |u| u.name.as_str());

            if troca.alvo.user_id == user_id && troca.status == super::StatusTroca::PendenteAlvo {
                trades_html.push_str(&format!(r#"
                    <div class="trade-item">
                        <p class="trade-details">
                            <span class="icon">📥</span> <strong>{}</strong> quer trocar um serviço consigo.
                        </p>
                        <p><i>Motivo: {}</i></p>
                        <div class="trade-actions">
                            <form action="/escala/responder_troca" method="post" style="display: inline-block;">
                                <input type="hidden" name="troca_id" value="{}">
                                <input type="hidden" name="acao" value="aprovar">
                                <button type="submit" class="btn btn-small-success">Aprovar</button>
                            </form>
                            <form action="/escala/responder_troca" method="post" style="display: inline-block;">
                                <input type="hidden" name="troca_id" value="{}">
                                <input type="hidden" name="acao" value="recusar">
                                <button type="submit" class="btn btn-small-danger">Recusar</button>
                            </form>
                        </div>
                    </div>
                "#, requerente_nome, troca.motivo, troca.id, troca.id));
            } else if troca.requerente.user_id == user_id {
                let (status_class, status_text) = match troca.status {
                    super::StatusTroca::PendenteAlvo => ("status-pending", format!("Aguardando {}", alvo_nome)),
                    super::StatusTroca::PendenteAdmin => ("status-pending", "Aguardando Escalante".to_string()),
                    super::StatusTroca::Aprovada => ("status-approved", "Aprovada".to_string()),
                    super::StatusTroca::Recusada => ("status-rejected", "Recusada".to_string()),
                };
                trades_html.push_str(&format!(r#"
                    <div class="trade-item">
                        <p class="trade-details">
                            <span class="icon">📤</span> Pedido enviado para <strong>{}</strong>
                        </p>
                        <p>Status: <span class="status-tag {}">{}</span></p>
                    </div>
                "#, alvo_nome, status_class, status_text));
            }
        }
        
        if trades_html.is_empty() {
            trades_html = "<p>Você não tem pedidos de troca pendentes ou em andamento.</p>".to_string();
        }

        format!(r#"<div>{trades_html}</div>"#)
    }

    pub async fn render_dashboard_page(state: &AppState, cookies: &Cookies) -> impl IntoResponse {
        let user_id = cookies.get("user_id").unwrap().value().to_string();
        
        let (user_name, user_roles_str, users_map) = {
            let users = state.users.lock().unwrap();
            let user = users.get(&user_id);
            let name = user.map(|u| u.name.clone()).unwrap_or_default();
            let roles = user.map(|u| if u.roles.is_empty() { "Utilizador Padrão".to_string() } else { u.roles.join(", ") }).unwrap_or_default();
            (name, roles, users.clone())
        };

        let form_state = super::meals::load_form_state().await.ok();
        let meal_status_closed = form_state.as_ref().map(|f| matches!(f.status, super::meals::FormStatus::Closed)).unwrap_or(false);
        
        let escala_estado = tokio::fs::read_to_string("data/escala/estado.json").await.ok()
            .and_then(|c| serde_json::from_str::<super::EstadoEscala>(&c).ok());
        let escala_period = escala_estado.as_ref().map(|e| (e.periodo_atual.start_date, e.periodo_atual.end_date));

        let (schedule_card, meals_card, trades_content) = tokio::join!(
            render_schedule_card(&user_id, escala_period),
            render_meals_card(&user_id),
            render_trades_content(&user_id, &users_map)
        );

        let mut buttons_html = String::new();
        if auth::has_role(state, cookies, "admin").await
            || auth::has_role(state, cookies, "polícia").await
            || auth::has_role(state, cookies, "chefe de dia").await
        {
            buttons_html.push_str(r#"<a href="/presence" class="btn btn-primary">📋 Controle de Presença</a>"#);
        }
        
        if meal_status_closed {
            buttons_html.push_str(r#"<a class="btn btn-primary" style="pointer-events: none; opacity: 0.5;">🍳 Municiamento (Fechado)</a>"#);
        } else {
            buttons_html.push_str(r#"<a href="/refeicoes" class="btn btn-primary">🍳 Municiamento</a>"#);
        }

        if auth::has_role(state, cookies, "rancheiro").await {
            buttons_html.push_str(r#"<a href="/admin/refeicoes" class="btn btn-primary">🔧 Admin Refeições</a>"#);
        }
        if auth::has_role(state, cookies, "rancheiro").await || auth::has_role(state, cookies, "conferência").await {
            buttons_html.push_str(r#"<a href="/refeicoes/checkin" class="btn btn-primary">✅ Conferir Refeições</a>"#);
        }
        if auth::has_role(state, cookies, "admin").await
            || auth::has_role(state, cookies, "escalante").await
        {
            buttons_html.push_str(r#"<a href="/admin" class="btn btn-accent">🔑 Admin Utilizadores</a>"#);
            buttons_html.push_str(r#"<a href="/admin/escala" class="btn btn-accent">🔧 Gerir Escalas</a>"#);
        }
        buttons_html.push_str(r#"<a href="/escala" class="btn btn-primary">📅 Consultar Escala</a>"#);

        let content = format!(r#"
            <header class="header">
                <div>
                    <h2>Bem-vindo(a), {user_name}!</h2>
                    <p style="color: var(--text-light); margin: 0;">Painel do Aluno</p>
                </div>
                <a href="/logout" class="btn">Sair</a>
            </header>
            <div class="dashboard-grid">
                <div class="main-column">
                    <div class="info-features-grid">
                        <div class="card">
                             <h2 class="card-title"><span class="icon">👤</span> Suas Informações</h2>
                             <p><strong>ID:</strong> {user_id}</p>
                             <p><strong>Função:</strong> {user_roles_str}</p>
                             <div style="margin-top: 20px; padding-top: 20px; border-top: 1px solid var(--border-color);">
                                <h3 style="margin-top: 0; font-size: 1.1em; font-weight: 500; display: flex; align-items: center;"><span class="icon" style="font-size: 1.2em;">🔄</span>Estado das Minhas Trocas</h3>
                                {trades_content}
                             </div>
                        </div>
                        <div class="card">
                            <h2 class="card-title"><span class="icon">🚀</span> Funcionalidades</h2>
                            <div class="features-grid">{buttons_html}</div>
                        </div>
                    </div>
                </div>
                <div class="sidebar-column">
                    {schedule_card}
                    {meals_card}
                </div>
            </div>
        "#);
        render_page("Dashboard", content, "")
    }
}

// --- HANDLERS (CONTROLADORES) ---

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

    if session_id.is_none() || user_id_cookie.is_none() || !state.sessions.lock().unwrap().contains(&session_id.unwrap()) {
        return Redirect::to("/").into_response();
    }
    
    view::render_dashboard_page(&state, &cookies).await.into_response()
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
