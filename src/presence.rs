// src/presence.rs

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;
use crate::auth::User;

// --- ALTERADO: Diretório e nome do ficheiro de dados ---
const DATA_DIR: &str = "data/presencas";
const PRESENCE_FILE: &str = "data/presencas/presenca.json";

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// --- NOVA STRUCT: Apenas os dados dinâmicos de presença ---
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PresenceEntry {
    pub ultima_saida: Option<DateTime<Local>>,
    pub ultimo_retorno: Option<DateTime<Local>>,
    pub usuario_saida: Option<String>,
    pub usuario_retorno: Option<String>,
}

// --- NOVA STRUCT: Combina dados estáticos (User) e dinâmicos (PresenceEntry) para a UI ---
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PresencePerson {
    pub id: String,
    pub curso: char,
    pub nome: String,
    pub ano: u8,
    pub ultima_saida: Option<DateTime<Local>>,
    pub ultimo_retorno: Option<DateTime<Local>>,
    pub usuario_saida: Option<String>,
    pub usuario_retorno: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PresenceStats {
    pub fora: usize,
    pub dentro: usize,
    pub total: usize,
}

/// Garante que o diretório de dados e o ficheiro presenca.json existam.
pub async fn ensure_presence_file() {
    if let Err(e) = fs::create_dir_all(DATA_DIR).await {
        eprintln!("🔥 Falha crítica ao criar o diretório '{}': {}", DATA_DIR, e);
        return;
    }

    if !fs::try_exists(PRESENCE_FILE).await.unwrap_or(false) {
        println!("📝 A criar ficheiro de presença em {}...", PRESENCE_FILE);
        // Cria um ficheiro com um mapa JSON vazio
        if let Err(e) = fs::write(PRESENCE_FILE, "{}").await {
            eprintln!("🔥 Falha crítica ao criar {}: {}", PRESENCE_FILE, e);
        }
    }
}

/// Carrega o mapa de presenças a partir do ficheiro JSON.
async fn load_presence_map() -> AppResult<HashMap<String, PresenceEntry>> {
    let content = fs::read_to_string(PRESENCE_FILE).await?;
    let presence_map: HashMap<String, PresenceEntry> = serde_json::from_str(&content)?;
    Ok(presence_map)
}

/// Guarda o mapa de presenças no ficheiro JSON.
async fn save_presence_map(presence_map: &HashMap<String, PresenceEntry>) -> AppResult<()> {
    let json_content = serde_json::to_string_pretty(presence_map)?;
    fs::write(PRESENCE_FILE, json_content).await?;
    Ok(())
}

/// Combina os dados de todos os utilizadores com os dados de presença para uma turma específica.
pub async fn get_presence_list_for_turma(
    all_users: &HashMap<String, User>,
    turma_num: u8,
) -> AppResult<Vec<PresencePerson>> {
    let presence_map = load_presence_map().await?;
    let mut presence_list = Vec::new();

    for user in all_users.values().filter(|u| u.ano == turma_num) {
        let entry = presence_map.get(&user.id).cloned().unwrap_or_default();
        presence_list.push(PresencePerson {
            id: user.id.clone(),
            curso: user.curso,
            nome: user.name.clone(),
            ano: user.ano,
            ultima_saida: entry.ultima_saida,
            ultimo_retorno: entry.ultimo_retorno,
            usuario_saida: entry.usuario_saida,
            usuario_retorno: entry.usuario_retorno,
        });
    }

    // Ordena a lista pelo ID do utilizador
    presence_list.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(presence_list)
}

/// Marca a saída de uma pessoa, atualizando o seu estado.
pub async fn marcar_saida(user_id: String, usuario_marcou: String) -> AppResult<()> {
    let mut presence_map = load_presence_map().await?;
    let entry = presence_map.entry(user_id).or_default();
    entry.ultima_saida = Some(Local::now());
    entry.usuario_saida = Some(usuario_marcou);
    save_presence_map(&presence_map).await
}

/// Marca o retorno de uma pessoa, atualizando o seu estado.
pub async fn marcar_retorno(user_id: String, usuario_marcou: String) -> AppResult<()> {
    let mut presence_map = load_presence_map().await?;
    let entry = presence_map.entry(user_id).or_default();
    entry.ultimo_retorno = Some(Local::now());
    entry.usuario_retorno = Some(usuario_marcou);
    save_presence_map(&presence_map).await
}

/// Calcula as estatísticas para um dado conjunto de pessoas.
pub fn calcular_stats(pessoas: &[PresencePerson]) -> PresenceStats {
    let fora = pessoas.iter().filter(|p| is_person_outside(p)).count();
    PresenceStats {
        fora,
        dentro: pessoas.len() - fora,
        total: pessoas.len(),
    }
}

/// Função auxiliar para determinar se uma pessoa está fora.
pub fn is_person_outside(pessoa: &PresencePerson) -> bool {
    match (&pessoa.ultima_saida, &pessoa.ultimo_retorno) {
        (Some(saida), Some(retorno)) => saida > retorno,
        (Some(_), None) => true,
        _ => false,
    }
}
