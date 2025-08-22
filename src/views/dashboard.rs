// src/views/dashboard.rs

// ADICIONADO: Importações necessárias com caminhos absolutos
use crate::auth::{self, AppState};
use crate::cautela::{self};
use crate::handlers::{DashboardMessage};
use axum::response::{Html, IntoResponse};
use chrono::{Datelike, Local, NaiveDate, Weekday};
use std::collections::{BTreeMap, HashMap};
use tokio_rusqlite::Connection;
use tower_cookies::Cookies;

// O conteúdo do `mod view` antigo vem para aqui.
// As funções que precisam ser chamadas de fora (login_page, render_dashboard_page)
// são marcadas com `pub`.

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
    
    input[type="text"], input[type="password"], textarea {
        width: 100%; padding: 14px; margin: 8px 0; border: 1px solid var(--border-color);
        border-radius: 4px; box-sizing: border-box; font-size: 16px;
    }
    
    .login-body { background: var(--background-color); }
    .login-container { max-width: 400px; margin: 10vh auto; }
    .login-card {
        background: var(--card-background);
        border-radius: 12px;
        box-shadow: 0 10px 25px rgba(0,0,0,0.1);
        padding: 60px;
        text-align: center;
    }
    .login-header h1 { margin: 0; font-size: 1.8em; color: var(--text-color); }
    .username-input { text-align: center; font-size: 1.2em; letter-spacing: 2px; }
    .password-container { display: flex; justify-content: center; gap: 10px; margin: 10px 0; cursor: text; outline: none; margin-bottom: 20px; }
    .password-box { width: 30px; height: 40px; border: 2px solid var(--border-color); border-radius: 8px; display: flex; align-items: center; justify-content: center; font-size: 1.5em; font-family: monospace; transition: border-color 0.2s; }
    .password-box.active { border-color: var(--primary-color); }
    .info-box { background: #f1f1f1; padding: 10px; border-radius: 6px; margin-top: 25px; font-size: 13px; color: var(--text-light); }
    
    .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem; }
    .item-list { list-style: none; padding: 0; margin: 0; }
    .item-list li { padding: 10px 0; border-bottom: 1px solid #f0f0f0; }
    .item-list li:last-child { border-bottom: none; }
    .features-grid { display: flex; flex-direction: column; gap: 12px; }
    .features-grid .btn { width: 100%; box-sizing: border-box; }
    .btn-small-success { background-color: var(--success-color); color: white; padding: 6px 12px; font-size: 12px;}
    .btn-small-danger { background-color: var(--danger-color); color: white; padding: 6px 12px; font-size: 12px;}
    .dashboard-grid { display: grid; grid-template-columns: 2fr 1fr; gap: 25px; align-items: start; }
    .main-column, .sidebar-column { display: flex; flex-direction: column; gap: 25px; }
    .info-features-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 25px; align-items: start; }
    @media (max-width: 1100px) { .info-features-grid { grid-template-columns: 1fr; } }
    @media (max-width: 900px) { .dashboard-grid { grid-template-columns: 1fr; } }
    .trade-item { border: 1px solid var(--border-color); border-radius: 6px; padding: 16px; margin-bottom: 12px; }
    .trade-item:last-child { margin-bottom: 0; }
    .trade-details { margin-bottom: 12px; font-size: 1em; }
    .trade-details .icon { color: var(--primary-color); margin-right: 8px; }
    .trade-actions { display: flex; gap: 10px; margin-top: 10px; }
    .status-tag { padding: 4px 10px; border-radius: 12px; font-size: 12px; font-weight: 500; color: white; text-transform: uppercase; display: inline-block; }
    .status-pending { background-color: #ffc107; }
    .status-approved { background-color: var(--success-color); }
    .status-rejected { background-color: var(--danger-color); }
    .schedule-day { display: flex; align-items: center; gap: 16px; padding: 12px 0; border-bottom: 1px solid var(--border-color); }
    .schedule-day:last-child { border-bottom: none; }
    .date-badge { flex-shrink: 0; width: 60px; height: 60px; background-color: #e8eaf6; border-radius: 8px; display: flex; flex-direction: column; align-items: center; justify-content: center; font-weight: 500; }
    .date-badge span:first-child { font-size: 1.1em; color: var(--primary-dark); }
    .date-badge span:last-child { font-size: 0.8em; text-transform: uppercase; color: var(--primary-color); }
    .service-info p { margin: 0; }
    
    .dashboard-message { padding-top: 20px; border-top: 1px solid var(--border-color); margin-top: 20px; }
    .dashboard-message-content.editable { cursor: pointer; }
    .dashboard-message-content p, .dashboard-message-content ul, .dashboard-message-content ol { margin: 0 0 10px 0; }
    .dashboard-message-content :last-child { margin-bottom: 0; }
    .dashboard-message-meta { font-size: 0.9em; color: var(--text-light); margin-top: 15px; text-align: right; }
    .editor-toolbar { background: #f1f1f1; padding: 8px; border-radius: 4px 4px 0 0; border: 1px solid var(--border-color); border-bottom: none; }
    .editor-toolbar button { padding: 6px 12px; margin-right: 5px; border: 1px solid transparent; background-color: #fff; cursor: pointer; font-family: 'Arial', sans-serif; }
    .editor-toolbar button:hover { background-color: #e0e0e0; }
    .editor-area { min-height: 120px; border: 1px solid var(--border-color); padding: 10px; outline: none; border-radius: 0 0 4px 4px;}
    .editor-area ul, .editor-area ol { margin-left: 20px; padding-left: 20px; }
"#;

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
        <body class="{body_class}"><div class="container">{content}</div></body>
        </html>
        "#,
    ))
}

pub fn login_page(error: Option<&str>) -> Html<String> {
    let error_html = error.map(|e| format!("<p style='color: var(--danger-color); text-align: center;'>{}</p>", e)).unwrap_or_default();
    let content = format!(
        r#"
        <div class="login-container"><div class="login-card">
            <div class="login-header"><h1>Área Restrita</h1></div>
            <form method="POST" action="/login">
                <input type="text" name="username" placeholder="Número Interno" required maxlength="4" class="username-input" />
                <input type="password" name="password" id="real-password-input" required style="display:none;">
                <div class="password-container" id="password-boxes" tabindex="0">
                    <div class="password-box"></div><div class="password-box"></div><div class="password-box"></div><div class="password-box"></div><div class="password-box"></div>
                </div>
                {error_html}
                <button type="submit" class="btn btn-primary btn-full">Entrar</button>
            </form>
            <div class="info-box"><strong>Versão do Sistema:</strong> 1.0 - OUT/2025</div>
        </div></div>
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
                    if (value.length <= 5) {{ password = value; updatePasswordDisplay(); }}
                    else {{ e.target.value = password; }}
                }});
                tempInput.addEventListener('blur', () => document.body.removeChild(tempInput));
            }};
            passwordContainer.addEventListener('click', activateInput);
            passwordContainer.addEventListener('focus', activateInput);
            function updatePasswordDisplay() {{
                realPasswordInput.value = password;
                passwordBoxes.forEach((box, index) => {{
                    if (index < password.length) {{ box.textContent = '●'; box.classList.remove('active'); }}
                    else {{ box.textContent = ''; box.classList.remove('active'); }}
                }});
                if (password.length < 5) {{ passwordBoxes[password.length].classList.add('active'); }}
            }}
            updatePasswordDisplay();
        </script>
        "#
    );
    render_page("Login", content, "login-body")
}

fn weekday_to_portuguese(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Seg", Weekday::Tue => "Ter", Weekday::Wed => "Qua",
        Weekday::Thu => "Qui", Weekday::Fri => "Sex", Weekday::Sat => "Sáb", Weekday::Sun => "Dom",
    }
}

 pub async fn render_cautela_card(user_id: &str) -> String {
     let conn = Connection::open(cautela::DB_FILE).await.unwrap();
     let user_id_owned = user_id.to_string();
     
     let user_loans_info: Vec<(String, String, NaiveDate)> = conn.call(move |conn| {
         let mut stmt = conn.prepare(
             "SELECT i.nome, e.exemplar_id, h.data_devolucao_prevista
             FROM emprestimos e
             JOIN exemplares ex ON e.exemplar_id = ex.numero_identificacao
             JOIN itens i ON ex.item_id = i.id
             JOIN historico_emprestimos h ON e.id = h.emprestimo_id
             WHERE e.status = 'Emprestado' AND e.aluno_id = ?1
             AND h.id = (SELECT MAX(id) FROM historico_emprestimos WHERE emprestimo_id = e.id)"
         )?;

         let mut loans = Vec::new();
         for row in stmt.query_map([&user_id_owned], |row| {
             Ok((row.get(0)?, row.get(1)?, NaiveDate::parse_from_str(&row.get::<_, String>(2)?, "%Y-%m-%d").unwrap()))
         })? {
             loans.push(row?);
         }
         Ok(loans)
     }).await.unwrap_or_default();

     let mut items_html = String::new();
     if user_loans_info.is_empty() {
         items_html = "<li>Você não possui itens emprestados.</li>".to_string();
     } else {
         for (item_name, exemplar_id, dev_date) in user_loans_info {
             let overdue_class = if dev_date < Local::now().date_naive() { "overdue" } else { "" };
             items_html.push_str(&format!(
                 "<li><span><strong>{}</strong> <small>#{}</small></span> <span class='{}'>Devolver até: {}</span></li>",
                 item_name, exemplar_id, overdue_class, dev_date.format("%d/%m/%Y")
             ));
         }
     }
     
     format!(r#"<div class="card"><h2 class="card-title"><span class="icon">📚</span> Meus Empréstimos</h2><ul class="item-list">{items_html}</ul></div>"#)
 }

pub async fn render_schedule_card(user_id: &str, escala_period: Option<(chrono::NaiveDate, chrono::NaiveDate)>) -> String {
    let Ok(estado_content) = tokio::fs::read_to_string("data/escala/estado.json").await else { return "".to_string(); };
    let Ok(estado) = serde_json::from_str::<crate::escala::EstadoEscala>(&estado_content) else { return "".to_string(); };
    
    let mut services_by_date: BTreeMap<chrono::NaiveDate, Vec<String>> = BTreeMap::new();
    let mut current_date = estado.periodo_atual.start_date;

    while current_date <= estado.periodo_atual.end_date {
        let filename = format!("{}/{}.json", crate::escala::ESCALA_DATA_DIR, current_date.format("%Y-%m-%d"));
        if let Ok(content) = tokio::fs::read_to_string(filename).await {
            if let Ok(escala_diaria) = serde_json::from_str::<crate::escala::EscalaDiaria>(&content) {
                for (posto, horarios) in &escala_diaria.escala {
                    for (_horario, alocacao) in horarios {
                        if alocacao.user_id == user_id {
                            let service_details = format!("<p><strong>{}</strong></p>", posto);
                            services_by_date.entry(current_date).or_default().push(service_details);
                        }
                    }
                }
            }
        }
        current_date = current_date.succ_opt().unwrap_or(current_date);
    }
    
    let services_html = if services_by_date.is_empty() {
        "<p>Você não está escalado para nenhum serviço no período ativo.</p>".to_string()
    } else {
        services_by_date.into_iter().map(|(date, services)| {
            format!(
                r#"<div class="schedule-day">
                    <div class="date-badge"><span>{dia}</span><span>{mes}</span></div>
                    <div class="service-info">{service_list}</div>
                </div>"#,
                dia = date.format("%d"),
                mes = weekday_to_portuguese(date.weekday()),
                service_list = services.join("")
            )
        }).collect()
    };

    let periodo_html = escala_period.map(|(start, end)| {
        format!("<div style='color: var(--text-light); font-size: 0.9em; margin-bottom: 10px; border-bottom: 1px solid var(--border-color); padding-bottom: 10px;'>Período: {} a {}</div>", start.format("%d/%m/%Y"), end.format("%d/%m/%Y"))
    }).unwrap_or_default();

    format!(r#"<div class="card"><h2 class="card-title"><span class="icon">📅</span> Meus Serviços</h2>{periodo_html}<div>{services_html}</div></div>"#)
}

pub async fn render_meals_card(user_id: &str) -> String {
    let Ok(form_state) = crate::meals::load_form_state().await else { return "".to_string() };
    let mut interests_html = String::new();
    let mut current_date = form_state.active_period.start_date;

    while current_date <= form_state.active_period.end_date {
        if let Ok(daily_data) = crate::meals::load_daily_meals(current_date).await {
            if let Some(selection) = daily_data.get(user_id) {
                let daily: Vec<&str> = [
                    (selection.cafe, "Café"), (selection.almoco, "Almoço"),
                    (selection.janta, "Jantar"), (selection.ceia, "Ceia"),
                ].iter().filter(|(sel, _)| *sel).map(|(_, name)| *name).collect();
                if !daily.is_empty() {
                    interests_html.push_str(&format!("<li><strong>{dia} ({data})</strong>: {refeicoes}</li>", dia = weekday_to_portuguese(current_date.weekday()), data = current_date.format("%d/%m"), refeicoes = daily.join(", ")));
                }
            }
        }
        current_date = current_date.succ_opt().unwrap_or(current_date);
    }
    
    if interests_html.is_empty() { interests_html = "<li>Nenhum interesse em refeições marcado.</li>".to_string(); }
    format!(r#"<div class="card"><h2 class="card-title"><span class="icon">🍳</span> Refeições</h2><ul class="item-list">{interests_html}</ul></div>"#)
}

pub async fn render_trades_content(user_id: &str, users_map: &HashMap<String, crate::auth::User>) -> String {
    let Ok(trocas_content) = tokio::fs::read_to_string("data/escala/trocas.json").await else { return "".to_string() };
    let Ok(todas_as_trocas) = serde_json::from_str::<Vec<crate::escala::Troca>>(&trocas_content) else { return "".to_string() };
    
    let mut trades_html = String::new();
    for troca in todas_as_trocas.iter().filter(|t| t.requerente.user_id == user_id || t.alvo.user_id == user_id) {
        let requerente_nome = users_map.get(&troca.requerente.user_id).map_or("N/A", |u| u.name.as_str());
        let alvo_nome = users_map.get(&troca.alvo.user_id).map_or("N/A", |u| u.name.as_str());
        if troca.alvo.user_id == user_id && troca.status == crate::escala::StatusTroca::PendenteAlvo {
            trades_html.push_str(&format!(r#"<div class="trade-item"><p class="trade-details"><span class="icon">📥</span> <strong>{}</strong> quer trocar um serviço consigo.</p><p><i>Motivo: {}</i></p><div class="trade-actions"><form action="/escala/responder_troca" method="post" style="display: inline-block;"><input type="hidden" name="troca_id" value="{}"><input type="hidden" name="acao" value="aprovar"><button type="submit" class="btn btn-small-success">Aprovar</button></form><form action="/escala/responder_troca" method="post" style="display: inline-block;"><input type="hidden" name="troca_id" value="{}"><input type="hidden" name="acao" value="recusar"><button type="submit" class="btn btn-small-danger">Recusar</button></form></div></div>"#, requerente_nome, troca.motivo, troca.id, troca.id));
        } else if troca.requerente.user_id == user_id {
            let (status_class, status_text) = match troca.status {
                crate::escala::StatusTroca::PendenteAlvo => ("status-pending", format!("Aguardando {}", alvo_nome)),
                crate::escala::StatusTroca::PendenteAdmin => ("status-pending", "Aguardando Escalante".to_string()),
                crate::escala::StatusTroca::Aprovada => ("status-approved", "Aprovada".to_string()),
                crate::escala::StatusTroca::Recusada => ("status-rejected", "Recusada".to_string()),
            };
            trades_html.push_str(&format!(r#"<div class="trade-item"><p class="trade-details"><span class="icon">📤</span> Pedido enviado para <strong>{}</strong></p><p>Status: <span class="status-tag {}">{}</span></p></div>"#, alvo_nome, status_class, status_text));
        }
    }
    
    if trades_html.is_empty() { trades_html = "<p>Você não tem pedidos de troca pendentes.</p>".to_string(); }
    format!(r#"<div>{trades_html}</div>"#)
}

pub async fn render_dashboard_page(
    state: &AppState,
    cookies: &Cookies,
    is_admin: bool,
    message: Option<DashboardMessage>,
) -> impl IntoResponse {
    let user_id = cookies.get("user_id").unwrap().value().to_string();
    let (user_name, user_roles_str, users_map) = {
        let users = state.users.lock().unwrap();
        let user = users.get(&user_id);
        let name = user.map(|u| u.name.clone()).unwrap_or_default();
        let roles = user.map(|u| if u.roles.is_empty() { "Utilizador Padrão".to_string() } else { u.roles.join(", ") }).unwrap_or_default();
        (name, roles, users.clone())
    };

    let form_state = crate::meals::load_form_state().await.ok();
    let meal_status_closed = form_state.as_ref().map(|f| matches!(f.status, crate::meals::FormStatus::Closed)).unwrap_or(true);
    let escala_estado = tokio::fs::read_to_string("data/escala/estado.json").await.ok().and_then(|c| serde_json::from_str::<crate::escala::EstadoEscala>(&c).ok());
    let escala_period = escala_estado.as_ref().map(|e| (e.periodo_atual.start_date, e.periodo_atual.end_date));

    let (schedule_card, meals_card, trades_content, cautela_card) = tokio::join!(
        render_schedule_card(&user_id, escala_period),
        render_meals_card(&user_id),
        render_trades_content(&user_id, &users_map),
        render_cautela_card(&user_id)
    );

    let mut buttons_html = String::new();
    if auth::has_role(state, cookies, "admin").await || auth::has_role(state, cookies, "polícia").await || auth::has_role(state, cookies, "chefe de dia").await {
        buttons_html.push_str(r#"<a href="/presence" class="btn btn-primary">📋 Controle de Presença</a>"#);
    }
    if meal_status_closed {
        buttons_html.push_str(r#"<a class="btn btn-primary" style="background-color: var(--text-light); cursor: not-allowed;">🍳 Municiamento (Fechado)</a>"#);
    } else {
        buttons_html.push_str(r#"<a href="/refeicoes" class="btn btn-primary">🍳 Municiamento</a>"#);
    }
    if auth::has_role(state, cookies, "rancheiro").await {
        buttons_html.push_str(r#"<a href="/admin/refeicoes" class="btn btn-primary">🔧 Admin Refeições</a>"#);
    }
    if auth::has_role(state, cookies, "rancheiro").await || auth::has_role(state, cookies, "conferência").await {
        buttons_html.push_str(r#"<a href="/refeicoes/checkin" class="btn btn-primary">✅ Conferir Refeições</a>"#);
    }
    if is_admin || auth::has_role(state, cookies, "escalante").await {
        buttons_html.push_str(r#"<a href="/admin" class="btn btn-accent">🔑 Admin Utilizadores</a>"#);
        buttons_html.push_str(r#"<a href="/admin/escala" class="btn btn-accent">🔧 Gerir Escalas</a>"#);
    }
    buttons_html.push_str(r#"<a href="/escala" class="btn btn-primary">📅 Consultar Escala</a>"#);

    let (message_content_html, message_meta_html, raw_content_for_editor) = if let Some(msg) = message {
        (msg.content.clone(), format!("<strong>{}</strong> - {} em {}", msg.author_role, msg.author_name, msg.timestamp.format("%d/%m")), msg.content)
    } else {
        ("<p>Nenhuma mensagem definida. Clique para adicionar uma.</p>".to_string(), "".to_string(), "".to_string())
    };
    
    let message_html = if is_admin || !raw_content_for_editor.is_empty() {
         let editable_class = if is_admin { "editable" } else { "" };
         format!(
            r#"
            <div class="dashboard-message" id="dashboard-message-container">
                <div id="message-display">
                    <div class="dashboard-message-content {editable_class}" id="message-content-display">{message_content_html}</div>
                    <p class="dashboard-message-meta" id="message-meta">{message_meta_html}</p>
                </div>
                <div id="message-edit-form" style="display:none;">
                    <form method="POST" action="/dashboard/update_message" id="message-form">
                        <input type="hidden" name="content" id="hidden-content-input">
                        <div class="editor-toolbar">
                            <button type="button" onclick="formatDoc('bold');"><b>B</b></button>
                            <button type="button" onclick="formatDoc('italic');"><i>I</i></button>
                            <button type="button" onclick="formatDoc('underline');"><u>U</u></button>
                            <button type="button" onclick="formatDoc('strikeThrough');"><s>S</s></button>
                            <button type="button" onclick="formatDoc('insertOrderedList');">1.</button>
                            <button type="button" onclick="formatDoc('insertUnorderedList');">•</button>
                            <button type="button" onclick="formatDoc('removeFormat');"> Limpar</button>
                        </div>
                        <div class="editor-area" id="rich-text-editor" contenteditable="true"></div>
                        <div style="text-align: right;">
                            <button type="button" class="btn" id="cancel-edit-btn" style="background-color: var(--text-light); color: white; margin-top: 10px;">Cancelar</button>
                            <button type="submit" class="btn btn-primary" style="margin-top: 10px;">Guardar</button>
                        </div>
                    </form>
                </div>
            </div>
            "#
        )
    } else { "".to_string() };

    let content = format!(r#"
        <header class="header">
            <div><h2>Bem-vindo(a), {user_name}!</h2><p style="color: var(--text-light); margin: 0;">Painel do Aluno</p></div>
            <a href="/logout" class="btn">Sair</a>
        </header>
        <div class="dashboard-grid">
            <div class="main-column">
                <div class="info-features-grid">
                    <div class="card">
                        <h2 class="card-title"><span class="icon">👤</span> Suas Informações</h2>
                        <p><strong>ID:</strong> {user_id}</p><p><strong>Função:</strong> {user_roles_str}</p>
                        {message_html}
                        <div style="margin-top: 20px; padding-top: 20px; border-top: 1px solid var(--border-color);">
                            <h3 style="margin-top: 0; font-size: 1.1em; font-weight: 500; display: flex; align-items: center;"><span class="icon" style="font-size: 1.2em;">🔄</span>Trocas Pendentes</h3>
                            {trades_content}
                        </div>
                    </div>
                    <div class="card">
                        <h2 class="card-title"><span class="icon">🚀</span> Funcionalidades</h2>
                        <div class="features-grid">{buttons_html}</div>
                    </div>
                </div>
            </div>
            <div class="sidebar-column">{schedule_card}{meals_card}{cautela_card}</div>
        </div>
        <script>
            const is_admin = {is_admin};
            if (is_admin) {{
                const displayDiv = document.getElementById('message-display');
                const editFormDiv = document.getElementById('message-edit-form');
                const contentDisplay = document.getElementById('message-content-display');
                const editor = document.getElementById('rich-text-editor');
                const hiddenInput = document.getElementById('hidden-content-input');
                const messageForm = document.getElementById('message-form');
                const cancelBtn = document.getElementById('cancel-edit-btn');
                const rawContent = `{raw_content_for_editor}`;
                if (contentDisplay) {{
                    contentDisplay.addEventListener('click', () => {{
                        editor.innerHTML = rawContent.replace(/\\`/g, '`');
                        displayDiv.style.display = 'none';
                        editFormDiv.style.display = 'block';
                        editor.focus();
                    }});
                }}
                if (cancelBtn) {{ cancelBtn.addEventListener('click', () => {{ displayDiv.style.display = 'block'; editFormDiv.style.display = 'none'; }}); }}
                window.formatDoc = function(command, value) {{ document.execCommand(command, false, value); editor.focus(); }};
                if (messageForm) {{ messageForm.addEventListener('submit', (e) => {{ hiddenInput.value = editor.innerHTML; }}); }}
            }}
        </script>
    "#, 
        user_name=user_name, user_id = user_id, user_roles_str = user_roles_str,
        is_admin = is_admin, message_html = message_html, trades_content = trades_content,
        buttons_html = buttons_html, schedule_card = schedule_card, meals_card = meals_card,
        cautela_card = cautela_card,
        raw_content_for_editor = raw_content_for_editor.replace('`', r#"\`"#).replace('\n', "")
    );
    render_page("Dashboard", content, "")
}