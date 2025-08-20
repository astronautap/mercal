// src/auth.rs

use crate::checkin::CheckinState;
use crate::escala::{self, EscalaDiaria, EstadoEscala};
use crate::presence_state::PresenceSocketState;
use chrono::{Local, Timelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::fs;
use tower_cookies::Cookies;
use crate::escala::Genero;

/// Representa o estado partilhado da aplicação.
#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<Mutex<HashSet<String>>>,
    pub users: Arc<Mutex<HashMap<String, User>>>,
    pub checkin_state: CheckinState,
    pub presence_state: PresenceSocketState,
}

/// Representa um utilizador do sistema.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub password: String,
    pub name: String,
    pub turma: String,
    pub ano: u8,
    pub curso: char,
    pub genero: Genero,
    pub roles: Vec<String>,
}

/// Estrutura para deserializar os dados do formulário de login.
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}


/// Verifica se um utilizador tem uma função, seja ela permanente ou temporária (do posto de serviço).
pub async fn has_role(state: &AppState, cookies: &Cookies, required_role: &str) -> bool {
    let user_id = match cookies.get("user_id") {
        Some(cookie) => cookie.value().to_string(),
        None => return false,
    };

    // --- CORRIGIDO: Usa to_lowercase() para uma comparação mais robusta ---
    let required_role_lower = required_role.to_lowercase();

    // 1. Verifica as funções permanentes
    {
        let users = state.users.lock().unwrap();
        if let Some(user) = users.get(&user_id) {
            if user.roles.iter().any(|role| role.to_lowercase() == required_role_lower) {
                return true;
            }
        }
    }

    // 2. Se não encontrou, verifica as funções temporárias da escala
    let now = Local::now();
    let today = now.date_naive();

    if let Ok(estado_content) = fs::read_to_string("data/escala/estado.json").await {
        if let Ok(estado) = serde_json::from_str::<EstadoEscala>(&estado_content) {
            
            if today < estado.periodo_atual.start_date || today > estado.periodo_atual.end_date {
                return false;
            }

            let service_date: NaiveDate = if now.hour() < 8 {
                today - Duration::days(1)
            } else {
                today
            };

            if service_date < estado.periodo_atual.start_date || service_date > estado.periodo_atual.end_date {
                 return false;
            }

            let filename = format!("{}/{}.json", escala::ESCALA_DATA_DIR, service_date.format("%Y-%m-%d"));
            
            if let Ok(content) = fs::read_to_string(filename).await {
                if let Ok(escala_diaria) = serde_json::from_str::<EscalaDiaria>(&content) {
                    for (posto_nome, horarios) in escala_diaria.escala {
                        if posto_nome.to_lowercase() == required_role_lower {
                            for alocacao in horarios.values() {
                                if alocacao.user_id == user_id {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    false
}
