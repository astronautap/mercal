// src/checkin_handlers.rs

//! # Handlers para o Check-in de Refeições
//!
//! Este módulo contém os handlers HTTP e WebSocket para a funcionalidade
//! de conferência de presença nas refeições em tempo real.

use crate::auth::{self, AppState, User};
use crate::checkin::{CheckinAction, CheckinState, CheckinUpdate};
use crate::meals::{self};
use axum::{
    debug_handler,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{header, StatusCode},
    response::{Html, IntoResponse},
};
use chrono::Local;
use futures_util::{stream::StreamExt, SinkExt};
use serde::Deserialize;
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use tower_cookies::Cookies;
use uuid::Uuid;

/// Helper para obter o nome do usuário logado a partir dos cookies.
fn get_current_user_name(state: &AppState, cookies: &Cookies) -> String {
    let user_id = cookies.get("user_id").map_or("Desconhecido".to_string(), |c| c.value().to_string());
    let users = state.users.lock().unwrap();
    users.get(&user_id)
        .map_or(user_id, |u| u.name.clone())
}

/// Apresenta a página de check-in de refeições com UI melhorada.
#[debug_handler]
pub async fn checkin_page(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    // --- CORRIGIDO: Adicionado .await e acento em "conferência" ---
    if !auth::has_role(&state, &cookies, "rancheiro").await && !auth::has_role(&state, &cookies, "conferência").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }

    let today = Local::now().date_naive();
    let daily_data = match meals::load_daily_meals(today).await {
        Ok(data) => data,
        Err(_) => return Html("<h1>Não foi possível carregar os dados das refeições para hoje.</h1><p>Verifique se o período de interesse foi aberto pelo rancheiro.</p><a href='/dashboard'>Voltar</a>").into_response(),
    };
    
    let all_users = state.users.lock().unwrap().clone();

    let mut meals_by_turma: BTreeMap<String, BTreeMap<String, Vec<User>>> = BTreeMap::new();
    for (user_id, selection) in &daily_data {
        if let Some(user) = all_users.get(user_id) {
            let turma_entry = meals_by_turma.entry(user.turma.clone()).or_default();
            if selection.cafe { turma_entry.entry("cafe".to_string()).or_default().push(user.clone()); }
            if selection.almoco { turma_entry.entry("almoco".to_string()).or_default().push(user.clone()); }
            if selection.janta { turma_entry.entry("janta".to_string()).or_default().push(user.clone()); }
            if selection.ceia { turma_entry.entry("ceia".to_string()).or_default().push(user.clone()); }
        }
    }

    let meals = ["cafe", "almoco", "janta", "ceia"];
    let meal_names = ["Café da Manhã", "Almoço", "Jantar", "Ceia"];
    let mut tab_buttons = String::new();
    let mut tab_content = String::new();

    for (i, meal) in meals.iter().enumerate() {
        let meal_name = meal_names[i];
        tab_buttons.push_str(&format!("<button class=\"tablink\" onclick=\"openMeal(event, '{}')\">{}</button>", meal, meal_name));
        
        let mut total_count = 0;
        let mut present_count = 0;
        let mut content = String::new();

        for (turma, meal_map) in &meals_by_turma {
            if let Some(users) = meal_map.get(*meal) {
                if !users.is_empty() {
                    content.push_str(&format!("<h3 class='turma-header'>Turma: {}</h3>", turma));
                    content.push_str("<ul class='user-list'>");
                    for user in users {
                        total_count += 1;
                        
                        let (realizado, marcador, hora) = if let Some(selection) = daily_data.get(&user.id) {
                            match *meal {
                                "cafe" => (selection.cafe_realizado, selection.cafe_marcado_por.as_deref(), selection.cafe_marcado_em.as_deref()),
                                "almoco" => (selection.almoco_realizado, selection.almoco_marcado_por.as_deref(), selection.almoco_marcado_em.as_deref()),
                                "janta" => (selection.janta_realizado, selection.janta_marcado_por.as_deref(), selection.janta_marcado_em.as_deref()),
                                "ceia" => (selection.ceia_realizado, selection.ceia_marcado_por.as_deref(), selection.ceia_marcado_em.as_deref()),
                                _ => (false, None, None),
                            }
                        } else {
                            (false, None, None)
                        };

                        if realizado { present_count += 1; }
                        let (row_class, btn_disabled) = if realizado { ("presente", "disabled") } else { ("", "") };
                        
                        let marker_html = match (marcador, hora) {
                            (Some(name), Some(time)) => format!("<span class='marker-info'>por {} às {}</span>", name, time),
                            (Some(name), None) => format!("<span class='marker-info'>por {}</span>", name),
                            _ => String::new(),
                        };

                        content.push_str(&format!(
                            "<li data-search-term='{} {}' class='user-item {}'>
                                <span class='user-info'><strong>{}</strong> - {}</span>
                                <div class='status-display'>
                                    <button class='checkin-btn' onclick=\"markPresent('{}', '{}')\" {}>Presente</button>
                                    {}
                                </div>
                             </li>",
                            user.id.to_lowercase(), user.name.to_lowercase(), row_class,
                            user.id, user.name,
                            user.id, meal, btn_disabled,
                            marker_html
                        ));
                    }
                    content.push_str("</ul>");
                }
            }
        }
        
        tab_content.push_str(&format!(
            "<div id='{}' class='tabcontent'>
                <div class='tab-header'>
                    <h2>Lista para o {}</h2>
                    <div class='header-actions'>
                        <span class='counter' id='counter-{}'>Presentes: {} / {}</span>
                        <a href='/refeicoes/checkin/relatorio_ausentes?meal={}' class='report-btn'>Gerar Relatório de Ausentes</a>
                    </div>
                </div>
                {}
             </div>",
            meal, meal_name, meal, present_count, total_count, meal, content
        ));
    }
    
    Html(format!(r##"
        <!DOCTYPE html>
        <html lang="pt-BR">
        <head>
            <title>Check-in de Refeições</title>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                :root {{ --primary-color: #007bff; --secondary-color: #6c757d; --success-color: #28a745; --light-gray: #f8f9fa; --border-color: #dee2e6; }}
                body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; margin: 0; background-color: #f4f7f9; color: #333; }}
                .container {{ max-width: 1200px; margin: 0 auto; padding: 20px; }}
                .header-bar {{ background-color: white; padding: 15px 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); position: sticky; top: 0; z-index: 1000; }}
                .header-content {{ display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 15px; }}
                h1 {{ margin: 0; font-size: 24px; }}
                .search-bar {{ padding: 10px; font-size: 16px; border: 1px solid var(--border-color); border-radius: 6px; width: 100%; max-width: 400px; }}
                .tab-container {{ display: flex; border-bottom: 1px solid var(--border-color); background-color: var(--light-gray); }}
                .tablink {{ background-color: transparent; flex: 1; border: none; outline: none; cursor: pointer; padding: 14px 16px; transition: all 0.3s; font-size: 16px; font-weight: 500; border-bottom: 3px solid transparent; }}
                .tablink:hover {{ background-color: #e9ecef; }}
                .tablink.active {{ border-bottom-color: var(--primary-color); color: var(--primary-color); }}
                .tabcontent {{ display: none; padding: 20px; background-color: white; }}
                .tab-header {{ display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 10px; border-bottom: 1px solid var(--border-color); padding-bottom: 10px; margin-bottom: 20px; }}
                .tab-header h2 {{ margin: 0; font-size: 20px; }}
                .header-actions {{ display: flex; align-items: center; gap: 15px; }}
                .counter {{ font-size: 16px; font-weight: 500; background-color: var(--light-gray); padding: 5px 10px; border-radius: 6px; }}
                .report-btn {{ background-color: var(--primary-color); color: white; text-decoration: none; padding: 8px 12px; border-radius: 5px; font-size: 14px; font-weight: 500; }}
                .turma-header {{ color: var(--primary-color); margin-top: 20px; margin-bottom: 10px; border-bottom: 1px solid var(--border-color); padding-bottom: 5px; }}
                .user-list {{ list-style-type: none; padding: 0; }}
                .user-item {{ display: flex; justify-content: space-between; align-items: center; padding: 15px; border-bottom: 1px solid #f0f0f0; transition: background-color 0.2s; }}
                .user-item:last-child {{ border-bottom: none; }}
                .user-info {{ font-size: 16px; }}
                .status-display {{ display: flex; align-items: center; gap: 10px; }}
                .checkin-btn {{ padding: 8px 16px; cursor: pointer; background-color: var(--success-color); color: white; border: none; border-radius: 5px; font-weight: 500; }}
                .checkin-btn:disabled {{ background-color: var(--secondary-color); cursor: not-allowed; }}
                .marker-info {{ font-size: 12px; color: #555; }}
                .user-item.presente {{ background-color: #d4edda; color: #155724; }}
                .user-item.presente .user-info {{ text-decoration: line-through; }}
                .dashboard-link {{ display: inline-block; margin-top: 20px; color: var(--primary-color); text-decoration: none; font-weight: 500; }}
            </style>
        </head>
        <body>
            <div class="header-bar">
                <div class="container">
                    <div class="header-content">
                        <h1>Check-in de Refeições ({})</h1>
                        <input type="text" id="searchInput" class="search-bar" onkeyup="filterUsers()" placeholder="Pesquisar por número ou nome...">
                    </div>
                </div>
                <div class="tab-container">{}</div>
            </div>
            <div class="container">
                {}
                <a href="/dashboard" class="dashboard-link">← Voltar ao Dashboard</a>
            </div>

            <script>
                // O JavaScript não precisa de alterações
                function openMeal(evt, mealName) {{
                    document.querySelectorAll(".tabcontent").forEach(tc => tc.style.display = "none");
                    document.querySelectorAll(".tablink").forEach(tl => tl.classList.remove("active"));
                    document.getElementById(mealName).style.display = "block";
                    evt.currentTarget.classList.add("active");
                    filterUsers();
                }}

                function filterUsers() {{
                    const input = document.getElementById("searchInput");
                    const filter = input.value.toLowerCase();
                    const activeTab = document.querySelector(".tabcontent[style*='block']");
                    if (!activeTab) return;

                    const items = activeTab.querySelectorAll(".user-item");
                    items.forEach(item => {{
                        const searchTerm = item.getAttribute('data-search-term');
                        if (searchTerm.includes(filter)) {{
                            item.style.display = "flex";
                        }} else {{
                            item.style.display = "none";
                        }}
                    }});
                }}

                const ws = new WebSocket(`ws://${{window.location.host}}/ws/refeicoes/checkin`);
                ws.onopen = () => console.log("WebSocket conectado.");
                ws.onmessage = function(event) {{
                    try {{
                        const update = JSON.parse(event.data);
                        const userRow = document.querySelector(`#${{update.meal}} .user-item[data-search-term*='${{update.user_id.toLowerCase()}}']`);
                        if (userRow && !userRow.classList.contains('presente')) {{
                            userRow.classList.add("presente");
                            const button = userRow.querySelector("button");
                            if (button) {{
                                button.disabled = true;
                            }}
                            
                            let statusDisplay = userRow.querySelector(".status-display");
                            if(statusDisplay){{
                                let markerSpan = statusDisplay.querySelector(".marker-info");
                                if(!markerSpan) {{
                                    markerSpan = document.createElement("span");
                                    markerSpan.className = "marker-info";
                                    statusDisplay.appendChild(markerSpan);
                                }}
                                markerSpan.textContent = `por ${{update.marked_by}} às ${{update.marked_at}}`;
                            }}

                            updateCounter(update.meal);
                        }}
                    }} catch (e) {{
                        console.error("Erro ao processar mensagem do servidor:", e);
                    }}
                }};

                function markPresent(userId, meal) {{
                    const action = {{ user_id: userId, meal: meal }};
                    ws.send(JSON.stringify(action));
                }}
                
                function updateCounter(meal) {{
                    const counterElement = document.getElementById(`counter-${{meal}}`);
                    if (!counterElement) return;
                    
                    let parts = counterElement.textContent.split('/');
                    let present = parseInt(parts[0].split(':')[1].trim(), 10);
                    let total = parseInt(parts[1].trim(), 10);
                    
                    present++;
                    counterElement.textContent = `Presentes: ${{present}} / ${{total}}`;
                }}

                document.addEventListener("DOMContentLoaded", () => {{
                    const firstTab = document.querySelector(".tablink");
                    if (firstTab) {{
                        firstTab.click();
                    }}
                }});
            </script>
        </body>
        </html>
    "##, today.format("%d/%m/%Y"), tab_buttons, tab_content)).into_response()
}

#[derive(Deserialize)]
pub struct ReportParams {
    meal: String,
}

#[debug_handler]
pub async fn generate_absent_report_handler(
    Query(params): Query<ReportParams>,
) -> impl IntoResponse {
    let today = Local::now().date_naive();
    let daily_data = match meals::load_daily_meals(today).await {
        Ok(data) => data,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Não foi possível carregar os dados das refeições.").into_response(),
    };

    let meal_name_pt = match params.meal.as_str() {
        "cafe" => "CAFÉ DA MANHÃ",
        "almoco" => "ALMOÇO",
        "janta" => "JANTAR",
        "ceia" => "CEIA",
        _ => "DESCONHECIDA",
    };

    let mut absent_by_turma: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut total_absent = 0;

    for (user_id, selection) in &daily_data {
        let is_absent = match params.meal.as_str() {
            "cafe" => selection.cafe && !selection.cafe_realizado,
            "almoco" => selection.almoco && !selection.almoco_realizado,
            "janta" => selection.janta && !selection.janta_realizado,
            "ceia" => selection.ceia && !selection.ceia_realizado,
            _ => false,
        };

        if is_absent {
            total_absent += 1;
            let user_info = format!("- {} - {}", user_id, selection.nome);
            absent_by_turma.entry(selection.turma.clone()).or_default().push(user_info);
        }
    }

    let mut report = String::new();
    report.push_str(&format!("RELATÓRIO DE AUSENTES - {}\n\n", meal_name_pt));
    report.push_str(&format!("Data: {}\n\n", today.format("%d/%m/%Y")));

    for (turma, users) in absent_by_turma {
        report.push_str(&format!("--- Turma: {} ---\n", turma));
        for user_line in users {
            report.push_str(&format!("{}\n", user_line));
        }
        report.push_str("\n");
    }

    report.push_str("----------------------------------\n");
    report.push_str(&format!("Total de Ausentes: {}\n", total_absent));
    report.push_str(&format!("Relatório gerado às {}\n", Local::now().format("%H:%M")));

    let filename = format!("Relatorio_Ausentes_{}_{}.txt", params.meal, today.format("%Y%m%d"));
    let headers = [
        (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
        (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", filename)),
    ];
    
    (headers, report).into_response()
}


/// Gere a conexão WebSocket para o check-in em tempo real.
#[debug_handler]
pub async fn checkin_websocket_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let operator_name = get_current_user_name(&state, &cookies);
    ws.on_upgrade(move |socket| handle_socket(socket, state.checkin_state, operator_name))
}

/// Função auxiliar para gerir o ciclo de vida de uma conexão WebSocket.
async fn handle_socket(socket: WebSocket, state: CheckinState, operator_name: String) {
    let (mut sender, mut receiver) = socket.split();
    
    let (tx, mut rx) = mpsc::channel(32);
    let conn_id = Uuid::new_v4().to_string();
    state.connections.lock().unwrap().insert(conn_id.clone(), tx);
    println!("Nova conexão WebSocket: {}", conn_id);

    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            if let Ok(action) = serde_json::from_str::<CheckinAction>(&text) {
                let today = Local::now().date_naive();
                if let Ok(mut daily_data) = meals::load_daily_meals(today).await {
                    if let Some(selection) = daily_data.get_mut(&action.user_id) {
                        
                        let (status_updated, marker_field, time_field) = match action.meal.as_str() {
                            "cafe" if !selection.cafe_realizado => (true, Some(&mut selection.cafe_marcado_por), Some(&mut selection.cafe_marcado_em)),
                            "almoco" if !selection.almoco_realizado => (true, Some(&mut selection.almoco_marcado_por), Some(&mut selection.almoco_marcado_em)),
                            "janta" if !selection.janta_realizado => (true, Some(&mut selection.janta_marcado_por), Some(&mut selection.janta_marcado_em)),
                            "ceia" if !selection.ceia_realizado => (true, Some(&mut selection.ceia_marcado_por), Some(&mut selection.ceia_marcado_em)),
                            _ => (false, None, None),
                        };

                        if status_updated {
                            let now_str = Local::now().format("%H:%M").to_string();
                            if let Some(field) = marker_field {
                                *field = Some(operator_name.clone());
                            }
                            if let Some(field) = time_field {
                                *field = Some(now_str.clone());
                            }
                            
                            match action.meal.as_str() {
                                "cafe" => selection.cafe_realizado = true,
                                "almoco" => selection.almoco_realizado = true,
                                "janta" => selection.janta_realizado = true,
                                "ceia" => selection.ceia_realizado = true,
                                _ => (),
                            }

                            if let Err(e) = meals::save_daily_meals(today, &daily_data).await {
                                eprintln!("Erro ao guardar check-in: {}", e);
                                continue;
                            }
                            
                            let update_msg = CheckinUpdate {
                                user_id: action.user_id.clone(),
                                meal: action.meal.clone(),
                                new_status: true,
                                marked_by: operator_name.clone(),
                                marked_at: now_str,
                            };
                            let broadcast_text = serde_json::to_string(&update_msg).unwrap();
                            state_clone.broadcast(broadcast_text).await;
                        }
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    state.connections.lock().unwrap().remove(&conn_id);
    println!("Conexão WebSocket {} fechada.", conn_id);
}
