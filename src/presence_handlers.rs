// src/presence_handlers.rs

use crate::auth::{self, AppState};
use crate::presence::{self};
use crate::presence_state::{PresenceSocketAction, PresenceSocketUpdate};
// ADICIONADO: Importar o novo módulo de views
use crate::views::presence as view;
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

// --- O MÓDULO 'VIEW' FOI REMOVIDO DAQUI ---

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
    if !auth::has_role(&state, &cookies, "admin").await
        && !auth::has_role(&state, &cookies, "polícia").await
        && !auth::has_role(&state, &cookies, "chefe de dia").await
    {
        return (
            StatusCode::FORBIDDEN,
            Html("<h1>Acesso Negado</h1><p>Esta funcionalidade é restrita.</p><a href='/dashboard'>Voltar</a>"),
        ).into_response();
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
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("<h1>Erro ao carregar dados</h1>"),
            )
                .into_response();
        }
    };
    
    let stats = presence::calcular_stats(&pessoas);
    
    // MODIFICADO: Chama a função da view e passa a função de formatação
    view::render_presence_page(turma_selecionada, &pessoas, &stats, &format_datetime_info).into_response()
}

#[debug_handler]
pub async fn presence_websocket_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let operator_id = cookies
        .get("user_id")
        .map_or("Desconhecido".to_string(), |c| c.value().to_string());
    ws.on_upgrade(move |socket| handle_socket(socket, state, operator_id))
}

async fn handle_socket(socket: WebSocket, state: AppState, operator_id: String) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel(32);
    let conn_id = Uuid::new_v4().to_string();
    state
        .presence_state
        .connections
        .lock()
        .unwrap()
        .insert(conn_id.clone(), tx);
    println!("Nova conexão WS de Presença: {}", conn_id);

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
            if let Ok(action) = serde_json::from_str::<PresenceSocketAction>(&text) {
                let (operator_name, user_to_update) = {
                    let users = state_clone.users.lock().unwrap();
                    let name = users
                        .get(&operator_id)
                        .map_or(operator_id.clone(), |u| u.name.clone());
                    let user = users.get(&action.user_id).cloned();
                    (name, user)
                };

                if user_to_update.is_none() {
                    let error_update = PresenceSocketUpdate {
                        success: false,
                        message: "Utilizador não encontrado.".to_string(),
                        ..Default::default()
                    };
                    state_clone
                        .presence_state
                        .broadcast(serde_json::to_string(&error_update).unwrap())
                        .await;
                    continue;
                }
                let user_to_update = user_to_update.unwrap();
                let turma_num = user_to_update.ano;

                let result = match action.action.as_str() {
                    "saida" => presence::marcar_saida(action.user_id.clone(), operator_name).await,
                    "retorno" => {
                        presence::marcar_retorno(action.user_id.clone(), operator_name).await
                    }
                    _ => Err("Ação inválida".into()),
                };

                let all_users = state_clone.users.lock().unwrap().clone();
                let pessoas_turma = presence::get_presence_list_for_turma(&all_users, turma_num)
                    .await
                    .unwrap_or_default();
                let stats = presence::calcular_stats(&pessoas_turma);

                let update_message = match result {
                    Ok(_) => {
                        if let Some(pessoa) = pessoas_turma.iter().find(|p| p.id == action.user_id)
                        {
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
                            PresenceSocketUpdate {
                                success: false,
                                message: "Pessoa não encontrada após atualização.".to_string(),
                                stats,
                                ..Default::default()
                            }
                        }
                    }
                    Err(e) => PresenceSocketUpdate {
                        success: false,
                        message: e.to_string(),
                        user_id: action.user_id,
                        stats,
                        ..Default::default()
                    },
                };
                
                state_clone
                    .presence_state
                    .broadcast(serde_json::to_string(&update_message).unwrap())
                    .await;
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
        (Some(data), Some(usuario)) => format!(
            "<span class='icon'>📅</span> {}<br><span class='icon'>👤</span> {}",
            data.format("%d/%m %H:%M"),
            usuario
        ),
        _ => "---".to_string(),
    };
    let retorno_info = match (&pessoa.ultimo_retorno, &pessoa.usuario_retorno) {
        (Some(data), Some(usuario)) => format!(
            "<span class='icon'>📅</span> {}<br><span class='icon'>👤</span> {}",
            data.format("%d/%m %H:%M"),
            usuario
        ),
        _ => "---".to_string(),
    };
    (saida_info, retorno_info)
}