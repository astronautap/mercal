// src/presence_state.rs

use crate::presence::PresenceStats;
use axum::extract::ws::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Representa o estado partilhado do serviço de WebSocket de presença.
#[derive(Clone, Default)]
pub struct PresenceSocketState {
    pub connections: Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>,
}

impl PresenceSocketState {
    /// Envia uma mensagem para todos os clientes conectados.
    pub async fn broadcast(&self, update_message: String) {
        let connections_to_notify: Vec<mpsc::Sender<Message>> = {
            let conns_map = self.connections.lock().unwrap();
            conns_map.values().cloned().collect()
        }; 

        let message = Message::Text(update_message.into());

        for tx in connections_to_notify {
            let _ = tx.send(message.clone()).await;
        }
    }
}

// --- ALTERADO: Usa user_id em vez de turma e pessoa ---
/// Mensagem enviada do cliente para o servidor (e.g., ao clicar "L" ou "R").
#[derive(Deserialize)]
pub struct PresenceSocketAction {
    pub user_id: String,
    pub action: String, // "saida" ou "retorno"
}

// --- ALTERADO: Usa user_id em vez de pessoa_numero ---
/// Mensagem enviada do servidor para todos os clientes para anunciar uma atualização.
#[derive(Serialize)]
pub struct PresenceSocketUpdate {
    pub success: bool,
    pub message: String,
    // Dados para atualizar a UI dinamicamente
    pub user_id: String,
    pub esta_fora: bool,
    pub saida_info_html: String,
    pub retorno_info_html: String,
    pub stats: PresenceStats,
}
