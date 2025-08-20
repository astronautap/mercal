// src/checkin.rs

//! # Módulo para Gestão do Check-in em Tempo Real (WebSockets)
//!
//! Este módulo define o estado partilhado que mantém o registo de todos os
//! operadores conectados e as estruturas de mensagens usadas na comunicação.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use axum::extract::ws::Message;

/// Representa o estado partilhado do serviço de check-in.
/// Mantém um mapa de todos os operadores conectados.
#[derive(Clone, Default)]
pub struct CheckinState {
    // Usamos um Arc<Mutex<...>> para partilhar o estado de forma segura entre threads.
    pub connections: Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>,
}

impl CheckinState {
    /// Envia uma mensagem de atualização para todos os operadores conectados.
    pub async fn broadcast(&self, update_message: String) {
        // --- LÓGICA CORRIGIDA ---
        // 1. Clona os 'senders' para liberar o lock o mais rápido possível.
        let connections_to_notify: Vec<mpsc::Sender<Message>> = {
            let conns_map = self.connections.lock().unwrap(); // Bloqueia
            conns_map.values().cloned().collect()
        }; // O bloqueio é liberado aqui

        let message = Message::Text(update_message);

        // 2. Itera sobre os clones e envia as mensagens de forma segura.
        for tx in connections_to_notify {
            // O .await agora é seguro, pois não há nenhum MutexGuard ativo.
            let _ = tx.send(message.clone()).await;
        }
    }
}

/// Mensagem enviada do cliente para o servidor quando um botão "Presente" é clicado.
#[derive(Deserialize)]
pub struct CheckinAction {
    pub user_id: String,
    pub meal: String, // "cafe", "almoco", "janta", "ceia"
}

/// Mensagem enviada do servidor para todos os clientes para anunciar uma atualização.
#[derive(Serialize)]
pub struct CheckinUpdate {
    pub user_id: String,
    pub meal: String,
    pub new_status: bool, // Sempre `true` neste caso
    pub marked_by: String,
    pub marked_at: String,
}
