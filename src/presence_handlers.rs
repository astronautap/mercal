// src/presence_handlers.rs

use crate::auth::{self, AppState};
use crate::presence::{self, PresencePerson};
use crate::presence_state::{PresenceSocketAction, PresenceSocketUpdate};
use axum::{
    debug_handler,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use futures_util::{stream::StreamExt, SinkExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tower_cookies::Cookies;
use uuid::Uuid;

// --- MÓDULO DE VISUALIZAÇÃO (HTML e CSS) ---
mod view {
    use super::{presence, PresencePerson};
    use axum::response::Html;

    // 1. CSS CENTRALIZADO E COM ESTILO MATERIAL DESIGN
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
            margin-bottom: 25px;
        }
        .header { text-align: center; margin-bottom: 2rem; }
        .header h1 { color: var(--primary-dark); }
        .turma-selector { display: flex; justify-content: center; gap: 10px; margin-bottom: 2rem; }
        .turma-btn {
            padding: 10px 24px; border: none; border-radius: 4px; text-decoration: none;
            font-weight: 500; cursor: pointer; transition: all 0.2s ease-in-out;
            background-color: var(--text-light); color: white;
        }
        .turma-btn.active { background-color: var(--primary-color); box-shadow: 0 2px 8px rgba(0,0,0,0.2); }
        .stats { display: flex; justify-content: center; gap: 20px; text-align: center; }
        .stat-number { font-size: 2.5em; font-weight: 700; }
        .stat-label { font-size: 1em; color: var(--text-light); }
        .stat-fora { color: var(--danger-color); }
        .stat-dentro { color: var(--success-color); }
        .stat-total { color: var(--primary-color); }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 12px 15px; text-align: left; border-bottom: 1px solid var(--border-color); }
        th { background-color: #f8f9fa; font-weight: 500; color: var(--primary-dark); }
        tr.fora { background-color: #fff1f2; }
        tr.dentro { background-color: #f0fff4; }
        .numero { font-weight: 500; width: 80px; }
        .acoes { width: 120px; text-align: center; }
        .info-saida, .info-retorno { width: 220px; font-size: 13px; color: var(--text-light); }
        .info-saida .icon, .info-retorno .icon { color: var(--primary-color); margin-right: 5px; }
        .btn-saida, .btn-retorno {
            padding: 6px 14px; margin: 0 3px; border: none; border-radius: 4px;
            cursor: pointer; font-weight: bold; color: white; transition: all 0.2s;
        }
        .btn-saida { background-color: #e57373; } .btn-saida:hover { background-color: #f44336; }
        .btn-retorno { background-color: #81c784; } .btn-retorno:hover { background-color: #4caf50; }
        .notification { position: fixed; top: 20px; right: 20px; padding: 15px; border-radius: 5px; color: white; z-index: 1000; display: none; box-shadow: 0 4px 10px rgba(0,0,0,0.2); }
        .notification.success { background: var(--success-color); }
        .notification.error { background: var(--danger-color); }
    "#;

    // 2. FUNÇÃO DE LAYOUT
    fn render_page(title: &str, content: String) -> Html<String> {
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
            <body>
                <div id="notification" class="notification"></div>
                <div class="container">
                    {content}
                </div>
            </body>
            </html>
            "#,
            title = title,
            CSS = CSS,
            content = content
        ))
    }
    
    // 3. FUNÇÃO DE RENDERIZAÇÃO DA PÁGINA DE PRESENÇA
    pub fn render_presence_page(
        turma_selecionada: u8,
        pessoas: &[PresencePerson],
        stats: &presence::PresenceStats,
    ) -> Html<String> {
        let mut pessoas_html = String::new();
        for pessoa in pessoas {
            let (saida_info, retorno_info) = super::format_datetime_info(pessoa);
            let status_class = if presence::is_person_outside(pessoa) { "fora" } else { "dentro" };
            
            pessoas_html.push_str(&format!(
                r#"
                <tr class="{status}" id="pessoa-{id}">
                    <td class="numero">{curso}{id}</td>
                    <td class="nome">{nome}</td>
                    <td class="acoes">
                        <button onclick="marcarPresenca('{id}', '{nome}', 'saida')" class="btn-saida">L</button>
                        <button onclick="marcarPresenca('{id}', '{nome}', 'retorno')" class="btn-retorno">R</button>
                    </td>
                    <td class="info-saida">{saida}</td>
                    <td class="info-retorno">{retorno}</td>
                </tr>
                "#,
                status = status_class,
                id = pessoa.id,
                curso = pessoa.curso,
                nome = pessoa.nome,
                saida = saida_info,
                retorno = retorno_info
            ));
        }

        let content = format!(
            r#"
            <div class="header"><h1>📋 Controle de Presença</h1></div>
            
            <div class="card">
                <div class="turma-selector">
                    <button class="turma-btn {active1}" onclick="selecionarTurma(1)">1º Ano</button>
                    <button class="turma-btn {active2}" onclick="selecionarTurma(2)">2º Ano</button>
                    <button class="turma-btn {active3}" onclick="selecionarTurma(3)">3º Ano</button>
                </div>
                <div class="stats" id="stats">
                    <div><div class="stat-number stat-fora" id="stat-fora">{fora}</div><div class="stat-label">Fora</div></div>
                    <div><div class="stat-number stat-dentro" id="stat-dentro">{dentro}</div><div class="stat-label">A Bordo</div></div>
                    <div><div class="stat-number stat-total" id="stat-total">{total}</div><div class="stat-label">Total</div></div>
                </div>
            </div>

            <div class="card">
                <table>
                    <thead>
                        <tr><th>Nº</th><th>Nome</th><th>Ações</th><th>Última Licença</th><th>Último Regresso</th></tr>
                    </thead>
                    <tbody id="pessoas-table">{pessoas_html}</tbody>
                </table>
            </div>
            <div style="text-align:center; margin-top: 20px;"><a href="/dashboard">← Voltar ao Dashboard</a></div>

            <script>
                function selecionarTurma(turma) {{ window.location.href = '/presence?turma=' + turma; }}

                function showNotification(message, type) {{
                    const notification = document.getElementById('notification');
                    notification.textContent = message;
                    notification.className = 'notification ' + type;
                    notification.style.display = 'block';
                    setTimeout(() => {{ notification.style.display = 'none'; }}, 3000);
                }}

                const ws = new WebSocket(`ws://${{window.location.host}}/ws/presence`);
                
                ws.onopen = () => console.log("WebSocket de Presença Conectado.");
                ws.onerror = () => showNotification("Erro de conexão com o servidor.", "error");

                ws.onmessage = function(event) {{
                    try {{
                        const update = JSON.parse(event.data);
                        if (!update.success) {{ showNotification(`Erro: ${{update.message}}`, 'error'); return; }}

                        document.getElementById('stat-fora').textContent = update.stats.fora;
                        document.getElementById('stat-dentro').textContent = update.stats.dentro;

                        const row = document.getElementById(`pessoa-${{update.user_id}}`);
                        if(row) {{
                            row.className = update.esta_fora ? 'fora' : 'dentro';
                            row.querySelector('.info-saida').innerHTML = update.saida_info_html;
                            row.querySelector('.info-retorno').innerHTML = update.retorno_info_html;
                        }}
                    }} catch(e) {{ console.error("Erro ao processar mensagem:", e); }}
                }};

                function marcarPresenca(userId, nome, action) {{
                    const actionText = action === 'saida' ? 'SAÍDA' : 'RETORNO';
                    if (!confirm(`Confirmar ${{actionText}} para ${{nome}} (${{userId}})?`)) return;

                    if (ws.readyState === WebSocket.OPEN) {{
                        const message = {{ user_id: userId, action: action }};
                        ws.send(JSON.stringify(message));
                    }} else {{
                        showNotification("A conexão não está ativa. Recarregue a página.", "error");
                    }}
                }}
            </script>
            "#,
            active1 = if turma_selecionada == 1 { "active" } else { "" },
            active2 = if turma_selecionada == 2 { "active" } else { "" },
            active3 = if turma_selecionada == 3 { "active" } else { "" },
            fora = stats.fora, dentro = stats.dentro, total = stats.total, 
            pessoas_html = pessoas_html
        );
        render_page("Controle de Presença", content)
    }
}

#[derive(Debug, Deserialize)]
pub struct PresenceQuery {
    turma: Option<u8>,
}

#[debug_handler]
pub async fn presence_page(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(params): Query<PresenceQuery>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await && !auth::has_role(&state, &cookies, "polícia").await && !auth::has_role(&state, &cookies, "chefe de dia").await {
        return (StatusCode::FORBIDDEN, Html("<h1>Acesso Negado</h1><p>Esta funcionalidade é restrita.</p><a href='/dashboard'>Voltar</a>")).into_response();
    }
    if cookies.get("session_id").is_none() {
        return Redirect::to("/").into_response();
    }

    let turma_selecionada = params.turma.unwrap_or(1);
    
    let all_users = state.users.lock().unwrap().clone();
    let pessoas = match presence::get_presence_list_for_turma(&all_users, turma_selecionada).await {
        Ok(lista) => lista,
        Err(e) => {
            eprintln!("Erro ao carregar lista de presença: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Html("<h1>Erro ao carregar dados</h1>")).into_response();
        }
    };
    
    let stats = presence::calcular_stats(&pessoas);
    
    // Usa a nova função de renderização do módulo `view`
    view::render_presence_page(turma_selecionada, &pessoas, &stats).into_response()
}

#[debug_handler]
pub async fn presence_websocket_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let operator_id = cookies.get("user_id").map_or("Desconhecido".to_string(), |c| c.value().to_string());
    ws.on_upgrade(move |socket| handle_socket(socket, state, operator_id))
}

async fn handle_socket(socket: WebSocket, state: AppState, operator_id: String) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel(32);
    let conn_id = Uuid::new_v4().to_string();
    state.presence_state.connections.lock().unwrap().insert(conn_id.clone(), tx);
    println!("Nova conexão WS de Presença: {}", conn_id);

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() { break; }
        }
    });

    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            if let Ok(action) = serde_json::from_str::<PresenceSocketAction>(&text) {
                let (operator_name, user_to_update) = {
                    let users = state_clone.users.lock().unwrap();
                    let name = users.get(&operator_id).map_or(operator_id.clone(), |u| u.name.clone());
                    let user = users.get(&action.user_id).cloned();
                    (name, user)
                };

                if user_to_update.is_none() {
                    let error_update = PresenceSocketUpdate { success: false, message: "Utilizador não encontrado.".to_string(), ..Default::default() };
                    state_clone.presence_state.broadcast(serde_json::to_string(&error_update).unwrap()).await;
                    continue;
                }
                let user_to_update = user_to_update.unwrap();
                let turma_num = user_to_update.ano;

                let result = match action.action.as_str() {
                    "saida" => presence::marcar_saida(action.user_id.clone(), operator_name).await,
                    "retorno" => presence::marcar_retorno(action.user_id.clone(), operator_name).await,
                    _ => Err("Ação inválida".into()),
                };

                let all_users = state_clone.users.lock().unwrap().clone();
                let pessoas_turma = presence::get_presence_list_for_turma(&all_users, turma_num).await.unwrap_or_default();
                let stats = presence::calcular_stats(&pessoas_turma);

                let update_message = match result {
                    Ok(_) => {
                        if let Some(pessoa) = pessoas_turma.iter().find(|p| p.id == action.user_id) {
                            let (saida_info, retorno_info) = format_datetime_info(pessoa);
                            PresenceSocketUpdate {
                                success: true,
                                message: "Ação registada com sucesso".to_string(),
                                user_id: action.user_id,
                                esta_fora: presence::is_person_outside(pessoa),
                                saida_info_html: saida_info,
                                retorno_info_html: retorno_info,
                                stats,
                            }
                        } else {
                            PresenceSocketUpdate { success: false, message: "Pessoa não encontrada após atualização.".to_string(), stats, ..Default::default() }
                        }
                    }
                    Err(e) => PresenceSocketUpdate { success: false, message: e.to_string(), user_id: action.user_id, stats, ..Default::default() }
                };
                
                state_clone.presence_state.broadcast(serde_json::to_string(&update_message).unwrap()).await;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    state.presence_state.connections.lock().unwrap().remove(&conn_id);
    println!("Conexão WS de Presença {} fechada.", conn_id);
}

// --- FUNÇÕES AUXILIARES ---

impl Default for PresenceSocketUpdate {
    fn default() -> Self {
        Self {
            success: false,
            message: String::new(),
            user_id: String::new(),
            esta_fora: false,
            saida_info_html: String::new(),
            retorno_info_html: String::new(),
            stats: Default::default(),
        }
    }
}

fn format_datetime_info(pessoa: &presence::PresencePerson) -> (String, String) {
    let saida_info = match (&pessoa.ultima_saida, &pessoa.usuario_saida) {
        (Some(data), Some(usuario)) => format!("<span class='icon'>📅</span> {}<br><span class='icon'>👤</span> {}", data.format("%d/%m %H:%M"), usuario),
        _ => "---".to_string(),
    };
    let retorno_info = match (&pessoa.ultimo_retorno, &pessoa.usuario_retorno) {
        (Some(data), Some(usuario)) => format!("<span class='icon'>📅</span> {}<br><span class='icon'>👤</span> {}", data.format("%d/%m %H:%M"), usuario),
        _ => "---".to_string(),
    };
    (saida_info, retorno_info)
}
