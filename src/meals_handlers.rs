// src/meals_handlers.rs

use crate::auth::{self, AppState};
use crate::meals::{self, AuditInfo, FormStatus, MealFormState, PeriodInfo};
// ADICIONADO: Importar o novo módulo de views
use crate::views;
use axum::{
    debug_handler,
    extract::{Form, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use chrono::{Datelike, Local, NaiveDate, Weekday};
use serde::Deserialize;
use std::collections::HashMap;
use tower_cookies::Cookies;

#[derive(Deserialize, Debug)]
pub struct AdminMealsForm {
    start_date: String,
    end_date: String,
}

// Função auxiliar para carregar o estado de forma segura ou criar um padrão
async fn get_or_create_form_state() -> MealFormState {
    match meals::load_form_state().await {
        Ok(state) => state,
        Err(e) => {
            eprintln!("AVISO: Falha ao carregar 'estado.json' (pode ser formato antigo): {}. A recriar com valores padrão.", e);
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
            if let Err(save_err) = meals::save_form_state(&default_state).await {
                eprintln!("ERRO CRÍTICO: Não foi possível recriar 'estado.json': {}", save_err);
            }
            default_state
        }
    }
}


/// Página de administração do formulário de refeições com layout melhorado.
#[debug_handler]
pub async fn admin_meals_page(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "rancheiro").await {
        return (
            StatusCode::FORBIDDEN,
            "Acesso negado. Apenas para rancheiros.",
        )
            .into_response();
    }

    let form_state = get_or_create_form_state().await;

    // LÓGICA MOVIDA DO HTML PARA O HANDLER
    let (status_html, actions_html, new_period_disabled) = match &form_state.status {
        FormStatus::Closed => (
            format!("<p><span class='status-closed'>FECHADO</span>. O período ativo é de {} a {}.</p>", 
                form_state.active_period.start_date.format("%d/%m/%Y"), form_state.active_period.end_date.format("%d/%m/%Y")),
            r#"<form method="POST" action="/admin/refeicoes/reopen" class="action-form">
                   <button type="submit" class="btn btn-warning">Reabrir Período Ativo para Edição</button>
               </form>"#.to_string(),
            false,
        ),
        FormStatus::PendingNew(pending) => (
            format!("<p><span class='status-open'>NOVO PERÍODO ABERTO</span> (De {} a {}) aguardando fecho.</p><p>O período ativo no dashboard continua a ser de {} a {}.</p>", 
                pending.start_date.format("%d/%m/%Y"), pending.end_date.format("%d/%m/%Y"),
                form_state.active_period.start_date.format("%d/%m/%Y"), form_state.active_period.end_date.format("%d/%m/%Y")),
            r#"<form method="POST" action="/admin/refeicoes/close" class="action-form">
                   <button type="submit" class="btn btn-danger">Fechar Novo Período e Torná-lo Ativo</button>
               </form>"#.to_string(),
            true,
        ),
        FormStatus::EditingActive => (
            format!("<p><span class='status-editing'>EM EDIÇÃO</span>. O período ativo ({} a {}) está aberto para alterações.</p>",
                form_state.active_period.start_date.format("%d/%m/%Y"), form_state.active_period.end_date.format("%d/%m/%Y")),
            r#"<form method="POST" action="/admin/refeicoes/save_edits" class="action-form">
                   <button type="submit" class="btn btn-success">Salvar Edições e Fechar Período</button>
               </form>"#.to_string(),
            true,
        ),
    };
    
    let daily_summary = meals::get_daily_summary_counts(form_state.active_period.start_date, form_state.active_period.end_date).await;

    let mut audit_html = String::new();
    if let Some(info) = &form_state.opened_info {
        audit_html.push_str(&format!("<li><strong>Aberto por:</strong> {} em {}</li>", info.by, info.at.format("%d/%m/%Y às %H:%M")));
    }
    if let Some(info) = &form_state.closed_info {
        audit_html.push_str(&format!("<li><strong>Fechado por:</strong> {} em {}</li>", info.by, info.at.format("%d/%m/%Y às %H:%M")));
    }
    if let Some(info) = &form_state.reopened_info {
        audit_html.push_str(&format!("<li><strong>Reaberto por:</strong> {} em {}</li>", info.by, info.at.format("%d/%m/%Y às %H:%M")));
    }

    // CHAMA A FUNÇÃO DA VIEW COM OS DADOS PRÉ-PROCESSADOS
    views::meals::admin_meals_page(
        status_html,
        actions_html,
        daily_summary,
        audit_html,
        new_period_disabled,
    ).into_response()
}

fn get_current_user_info(state: &AppState, cookies: &Cookies) -> String {
    let user_id = cookies
        .get("user_id")
        .map_or("unknown".to_string(), |c| c.value().to_string());
    let users = state.users.lock().unwrap();
    users
        .get(&user_id)
        .map_or(user_id.clone(), |u| u.name.clone())
}

#[debug_handler]
pub async fn open_meals_form(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<AdminMealsForm>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "rancheiro").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }
    
    let mut form_state = get_or_create_form_state().await;
    if !matches!(form_state.status, FormStatus::Closed) {
        return (
            StatusCode::BAD_REQUEST,
            "Só pode abrir um novo período se o sistema estiver fechado.",
        )
            .into_response();
    }

    let start_date = NaiveDate::parse_from_str(&form.start_date, "%Y-%m-%d").unwrap();
    let end_date = NaiveDate::parse_from_str(&form.end_date, "%Y-%m-%d").unwrap();

    if start_date > end_date {
        return (
            StatusCode::BAD_REQUEST,
            "A data de início não pode ser posterior à data de fim.",
        )
            .into_response();
    }

    let new_pending_period = PeriodInfo { start_date, end_date };
    form_state.status = FormStatus::PendingNew(new_pending_period);
    form_state.opened_info = Some(AuditInfo {
        by: get_current_user_info(&state, &cookies),
        at: Local::now(),
    });
    form_state.closed_info = None;
    form_state.reopened_info = None;

    let users_clone = state.users.lock().unwrap().clone();
    if let Err(e) = meals::create_daily_meal_files(start_date, end_date, &users_clone).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Erro ao criar ficheiros de refeição: {}", e),
        )
            .into_response();
    }
    if let Err(e) = meals::save_form_state(&form_state).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Erro ao guardar estado: {}", e),
        )
            .into_response();
    }
    Redirect::to("/admin/refeicoes").into_response()
}

#[debug_handler]
pub async fn close_meals_form(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "rancheiro").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }
    
    let mut form_state = get_or_create_form_state().await;
    
    if let FormStatus::PendingNew(pending) = form_state.status {
        let old_active_period = std::mem::replace(&mut form_state.active_period, pending);
        form_state.status = FormStatus::Closed;
        form_state.closed_info = Some(AuditInfo {
            by: get_current_user_info(&state, &cookies),
            at: Local::now(),
        });
        
        let deletion_start = old_active_period.start_date;
        let deletion_end = if let Some(day_before_new) = form_state.active_period.start_date.pred_opt() {
            std::cmp::min(old_active_period.end_date, day_before_new)
        } else {
            old_active_period.end_date
        };

        if deletion_start <= deletion_end {
            if let Err(e) = meals::delete_daily_meal_files(deletion_start, deletion_end).await {
                eprintln!("AVISO: Falha ao apagar ficheiros do período antigo: {}", e);
            }
        }

        if let Err(e) = meals::save_form_state(&form_state).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Erro ao guardar estado: {}", e),
            )
                .into_response();
        }
    }
    Redirect::to("/admin/refeicoes").into_response()
}

#[debug_handler]
pub async fn reopen_active_period_form(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "rancheiro").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }
    
    let mut form_state = get_or_create_form_state().await;
    if matches!(form_state.status, FormStatus::Closed) {
        let users_clone = state.users.lock().unwrap().clone();
        let active_period = &form_state.active_period;
        if let Err(e) =
            meals::create_daily_meal_files(active_period.start_date, active_period.end_date, &users_clone)
                .await
        {
            eprintln!(
                "AVISO: Falha ao verificar/criar ficheiros de refeição ao reabrir: {}",
                e
            );
        }

        form_state.status = FormStatus::EditingActive;
        form_state.reopened_info = Some(AuditInfo {
            by: get_current_user_info(&state, &cookies),
            at: Local::now(),
        });
        if let Err(e) = meals::save_form_state(&form_state).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Erro ao guardar estado: {}", e),
            )
                .into_response();
        }
    }
    Redirect::to("/admin/refeicoes").into_response()
}

#[debug_handler]
pub async fn save_edits_form(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "rancheiro").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }
    
    let mut form_state = get_or_create_form_state().await;
    if matches!(form_state.status, FormStatus::EditingActive) {
        form_state.status = FormStatus::Closed;
        form_state.closed_info = Some(AuditInfo {
            by: get_current_user_info(&state, &cookies),
            at: Local::now(),
        });
        if let Err(e) = meals::save_form_state(&form_state).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Erro ao guardar estado: {}", e),
            )
                .into_response();
        }
    }
    Redirect::to("/admin/refeicoes").into_response()
}

#[debug_handler]
pub async fn user_meals_page(
    State(_state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let user_id = cookies
        .get("user_id")
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    
    let form_state = get_or_create_form_state().await;

    let period_to_show = match form_state.status {
        FormStatus::PendingNew(period) => Some(period),
        FormStatus::EditingActive => Some(form_state.active_period),
        FormStatus::Closed => None,
    };

    if let Some(period) = period_to_show {
        let mut day_cards_html = String::new();
        let mut current_date = period.start_date;

        while current_date <= period.end_date {
            let daily_data = meals::load_daily_meals(current_date).await.ok();
            let selection = daily_data.as_ref().and_then(|d| d.get(&user_id));
            
            let date_str = current_date.format("%Y-%m-%d");
            let mut meal_options_html = String::new();
            for (meal, emoji, label) in [
                ("cafe", "☕", "Café"),
                ("almoco", "🍛", "Almoço"),
                ("janta", "🍲", "Jantar"),
                ("ceia", "🌙", "Ceia"),
            ] {
                let is_checked = selection.map_or(false, |s| match meal {
                    "cafe" => s.cafe,
                    "almoco" => s.almoco,
                    "janta" => s.janta,
                    "ceia" => s.ceia,
                    _ => false,
                });
                meal_options_html.push_str(&format!(
                    r#"<div class="meal-toggle">
                        <input type="checkbox" id="{m}-{d}" name="{m}-{d}" {c}>
                        <label for="{m}-{d}">{e} {l}</label>
                    </div>"#,
                    m = meal,
                    d = date_str,
                    c = if is_checked { "checked" } else { "" },
                    e = emoji,
                    l = label
                ));
            }

            day_cards_html.push_str(&format!(
                r#"<div class="day-card">
                    <h3>{} <span>{}</span></h3>
                    <div class="meal-options">{}</div>
                </div>"#,
                current_date.format("%d/%m/%Y"),
                weekday_to_portuguese(current_date.weekday()),
                meal_options_html
            ));

            current_date = current_date.succ_opt().unwrap_or(current_date);
        }
        
        // CHAMA A FUNÇÃO DA VIEW
        views::meals::user_meals_page(&period, day_cards_html)
    } else {
        views::meals::user_meals_page(&PeriodInfo::default(), String::new())
    }
}

#[debug_handler]
pub async fn save_all_meals_handler(
    State(_state): State<AppState>,
    cookies: Cookies,
    Form(form_data): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id = cookies
        .get("user_id")
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    
    let form_state = get_or_create_form_state().await;
    
    let period_to_save = match form_state.status {
        FormStatus::PendingNew(period) => Some(period),
        FormStatus::EditingActive => Some(form_state.active_period),
        _ => None,
    };

    if let Some(period) = period_to_save {
        let mut current_date = period.start_date;
        while current_date <= period.end_date {
            if let Ok(mut daily_data) = meals::load_daily_meals(current_date).await {
                if let Some(selection) = daily_data.get_mut(&user_id) {
                    let date_str = current_date.format("%Y-%m-%d");
                    selection.cafe = form_data.contains_key(&format!("cafe-{}", date_str));
                    selection.almoco = form_data.contains_key(&format!("almoco-{}", date_str));
                    selection.janta = form_data.contains_key(&format!("janta-{}", date_str));
                    selection.ceia = form_data.contains_key(&format!("ceia-{}", date_str));
                    
                    let _ = meals::save_daily_meals(current_date, &daily_data).await;
                }
            }
            current_date = current_date.succ_opt().unwrap_or(current_date);
        }
    }
    
    Redirect::to("/refeicoes").into_response()
}

fn weekday_to_portuguese(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Segunda",
        Weekday::Tue => "Terça",
        Weekday::Wed => "Quarta",
        Weekday::Thu => "Quinta",
        Weekday::Fri => "Sexta",
        Weekday::Sat => "Sábado",
        Weekday::Sun => "Domingo",
    }
}