// src/meals.rs

use crate::auth::User;
use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tokio::fs;

const MEALS_DATA_DIR: &str = "data/refeicoes";
const STATE_FILE: &str = "data/refeicoes/estado.json";

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct PeriodInfo {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

impl Default for PeriodInfo {
    fn default() -> Self {
        Self {
            start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditInfo {
    pub by: String,
    pub at: DateTime<Local>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FormStatus {
    Closed,
    PendingNew(PeriodInfo),
    EditingActive,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MealFormState {
    pub active_period: PeriodInfo,
    pub status: FormStatus,
    pub opened_info: Option<AuditInfo>,
    pub closed_info: Option<AuditInfo>,
    pub reopened_info: Option<AuditInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MealSelection {
    pub nome: String,
    pub turma: String,
    pub cafe: bool,
    pub almoco: bool,
    pub janta: bool,
    pub ceia: bool,
    #[serde(default)]
    pub cafe_realizado: bool,
    #[serde(default)]
    pub almoco_realizado: bool,
    #[serde(default)]
    pub janta_realizado: bool,
    #[serde(default)]
    pub ceia_realizado: bool,
    #[serde(default)]
    pub cafe_marcado_por: Option<String>,
    #[serde(default)]
    pub cafe_marcado_em: Option<String>,
    #[serde(default)]
    pub almoco_marcado_por: Option<String>,
    #[serde(default)]
    pub almoco_marcado_em: Option<String>,
    #[serde(default)]
    pub janta_marcado_por: Option<String>,
    #[serde(default)]
    pub janta_marcado_em: Option<String>,
    #[serde(default)]
    pub ceia_marcado_por: Option<String>,
    #[serde(default)]
    pub ceia_marcado_em: Option<String>,
}

#[derive(Default)]
pub struct MealSummary {
    pub cafe: u32,
    pub almoco: u32,
    pub janta: u32,
    pub ceia: u32,
}

pub async fn ensure_meals_structure() {
    if let Err(e) = fs::create_dir_all(MEALS_DATA_DIR).await {
        eprintln!("🔥 Falha crítica ao criar o diretório '{}': {}", MEALS_DATA_DIR, e);
    }
    if fs::try_exists(STATE_FILE).await.unwrap_or(false) {
        return;
    }
    let default_state = MealFormState {
        active_period: PeriodInfo {
            start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        },
        status: FormStatus::Closed,
        opened_info: None,
        closed_info: None,
        reopened_info: None,
    };
    if let Ok(json) = serde_json::to_string_pretty(&default_state) {
        let _ = fs::write(STATE_FILE, json).await;
    }
}

pub async fn load_form_state() -> AppResult<MealFormState> {
    let content = fs::read_to_string(STATE_FILE).await?;
    let state: MealFormState = serde_json::from_str(&content)?;
    Ok(state)
}

pub async fn save_form_state(state: &MealFormState) -> AppResult<()> {
    let json_content = serde_json::to_string_pretty(state)?;
    fs::write(STATE_FILE, json_content).await?;
    Ok(())
}

/// Cria os ficheiros de refeição diários, **sem sobrescrever os que já existem**.
pub async fn create_daily_meal_files(start: NaiveDate, end: NaiveDate, users: &HashMap<String, User>) -> AppResult<()> {
    let mut current_date = start;
    while current_date <= end {
        let filename = format!("{}/{}.json", MEALS_DATA_DIR, current_date.format("%Y-%m-%d"));
        
        // --- CORREÇÃO ---
        // Apenas cria o ficheiro se ele não existir, para não apagar dados ao reabrir.
        if fs::try_exists(&filename).await? {
            current_date = current_date.succ_opt().unwrap_or(current_date);
            continue; // Pula para o próximo dia
        }
        
        let mut daily_data: HashMap<String, MealSelection> = HashMap::new();
        for user in users.values() {
            daily_data.insert(
                user.id.clone(),
                MealSelection {
                    nome: user.name.clone(),
                    turma: user.turma.clone(),
                    cafe: false,
                    almoco: false,
                    janta: false,
                    ceia: false,
                    cafe_realizado: false,
                    almoco_realizado: false,
                    janta_realizado: false,
                    ceia_realizado: false,
                    cafe_marcado_por: None,
                    cafe_marcado_em: None,
                    almoco_marcado_por: None,
                    almoco_marcado_em: None,
                    janta_marcado_por: None,
                    janta_marcado_em: None,
                    ceia_marcado_por: None,
                    ceia_marcado_em: None,
                },
            );
        }
        let json_content = serde_json::to_string_pretty(&daily_data)?;
        fs::write(filename, json_content).await?;
        current_date = current_date.succ_opt().unwrap_or(current_date);
    }
    Ok(())
}

pub async fn delete_daily_meal_files(start: NaiveDate, end: NaiveDate) -> AppResult<()> {
    let mut current_date = start;
    while current_date <= end {
        let filename = format!("{}/{}.json", MEALS_DATA_DIR, current_date.format("%Y-%m-%d"));
        if fs::try_exists(&filename).await? {
            fs::remove_file(filename).await?;
        }
        current_date = current_date.succ_opt().unwrap_or(current_date);
    }
    Ok(())
}

pub async fn load_daily_meals(date: NaiveDate) -> AppResult<HashMap<String, MealSelection>> {
    let filename = format!("{}/{}.json", MEALS_DATA_DIR, date.format("%Y-%m-%d"));
    let content = fs::read_to_string(filename).await?;
    let data = serde_json::from_str(&content)?;
    Ok(data)
}

pub async fn save_daily_meals(date: NaiveDate, data: &HashMap<String, MealSelection>) -> AppResult<()> {
    let filename = format!("{}/{}.json", MEALS_DATA_DIR, date.format("%Y-%m-%d"));
    let json_content = serde_json::to_string_pretty(data)?;
    fs::write(filename, json_content).await?;
    Ok(())
}

pub async fn get_daily_summary_counts(start: NaiveDate, end: NaiveDate) -> BTreeMap<NaiveDate, MealSummary> {
    let mut daily_summary = BTreeMap::new();
    let mut current_date = start;

    while current_date <= end {
        let mut summary_for_day = MealSummary::default();
        if let Ok(daily_data) = load_daily_meals(current_date).await {
            for selection in daily_data.values() {
                if selection.cafe { summary_for_day.cafe += 1; }
                if selection.almoco { summary_for_day.almoco += 1; }
                if selection.janta { summary_for_day.janta += 1; }
                if selection.ceia { summary_for_day.ceia += 1; }
            }
        }
        daily_summary.insert(current_date, summary_for_day);
        current_date = current_date.succ_opt().unwrap_or(current_date);
    }

    daily_summary
}
