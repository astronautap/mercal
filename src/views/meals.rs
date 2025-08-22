// src/views/meals.rs

use crate::meals::{MealSummary, PeriodInfo};
use axum::response::{Html, IntoResponse};
use chrono::{Datelike, NaiveDate, Weekday};
use std::collections::BTreeMap;

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

/// Página de administração do formulário de refeições com layout melhorado.
pub fn admin_meals_page(
    status_html: String,
    actions_html: String,
    daily_summary: BTreeMap<NaiveDate, MealSummary>,
    audit_html: String,
    new_period_disabled: bool,
) -> impl IntoResponse {
    let mut summary_html = String::new();
    for (date, counts) in daily_summary {
        let weekday_pt = weekday_to_portuguese(date.weekday());
        summary_html.push_str(&format!(
            "<div class='day-summary-card'><h5>{}<br>{}</h5><ul><li>Café: <strong>{}</strong></li><li>Almoço: <strong>{}</strong></li><li>Janta: <strong>{}</strong></li><li>Ceia: <strong>{}</strong></li></ul></div>",
            weekday_pt, date.format("%d/%m/%Y"), counts.cafe, counts.almoco, counts.janta, counts.ceia
        ));
    }

    Html(format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Admin - Gestão de Refeições</title>
            <style>
                body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; max-width: 1200px; margin: 40px auto; padding: 20px; background-color: #f4f7f9; color: #333; }}
                .card {{ background: white; border: 1px solid #e0e0e0; padding: 25px; border-radius: 8px; margin-bottom: 25px; box-shadow: 0 4px 6px rgba(0,0,0,0.05); }}
                .btn {{ display: inline-block; padding: 12px 20px; border-radius: 6px; text-decoration: none; color: white; border: none; cursor: pointer; font-size: 16px; font-weight: 500; transition: background-color 0.2s; }}
                .btn-danger {{ background-color: #dc3545; }} .btn-danger:hover {{ background-color: #c82333; }}
                .btn-primary {{ background-color: #007bff; }} .btn-primary:hover {{ background-color: #0056b3; }}
                .btn-warning {{ background-color: #ffc107; color: black; }} .btn-warning:hover {{ background-color: #e0a800; }}
                .btn-success {{ background-color: #28a745; }} .btn-success:hover {{ background-color: #218838; }}
                button:disabled {{ background-color: #6c757d; cursor: not-allowed; }}
                .summary-container {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 15px; margin-top: 20px; }}
                .day-summary-card {{ background-color: #f8f9fa; padding: 15px; border-radius: 8px; border: 1px solid #dee2e6; }}
                .day-summary-card h5 {{ margin-top: 0; }}
                .status-open {{ color: #28a745; }} .status-closed {{ color: #dc3545; }} .status-editing {{ color: #ffc107; }}
                .text-muted {{ color: #6c757d; font-size: 14px; }}
                .action-form {{ margin-top: 15px; }}
                .audit-list {{ list-style-type: none; padding-left: 0; font-size: 14px; color: #555; }}
            </style>
        </head>
        <body>
            <h1>Painel de Controlo de Refeições</h1>
            <div class="card">
                <h2>Situação do Período</h2>
                {}
                <h3>Ações</h3>
                {}
            </div>
            <div class="card">
                <h3>Resumo do Período Ativo</h3>
                <div class="summary-container">{}</div>
            </div>
            <div class="card">
                <h3>Histórico de Ações</h3>
                <ul class="audit-list">{}</ul>
            </div>
            <div class="card">
                <h3>Abrir Novo Período de Interesse</h3>
                <form method="POST" action="/admin/refeicoes/open">
                    <fieldset {}>
                        <label for="start_date">Data de Início:</label>
                        <input type="date" id="start_date" name="start_date" required><br><br>
                        <label for="end_date">Data de Fim:</label>
                        <input type="date" id="end_date" name="end_date" required>
                        <p class="text-muted">Isto irá abrir um novo período para os utilizadores preencherem, sem apagar o período ativo atual.</p>
                        <button type="submit" class="btn btn-primary">Abrir Novo Período</button>
                    </fieldset>
                </form>
            </div>
            <a href="/dashboard">← Voltar ao Dashboard</a>
        </body>
        </html>
        "#,
        status_html, actions_html, summary_html, audit_html, if new_period_disabled { "disabled" } else { "" }
    ))
    .into_response()
}

/// Página para o utilizador marcar as suas refeições.
pub fn user_meals_page(period: &PeriodInfo, day_cards_html: String) -> impl IntoResponse {
    if day_cards_html.is_empty() {
        return Html("<h1>Período de marcação de refeições está fechado.</h1><a href='/dashboard'>Voltar ao Dashboard</a>").into_response();
    }

    Html(format!(
        r#"
        <!DOCTYPE html>
        <html lang="pt-BR">
        <head>
            <title>Marcação de Refeições</title>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; background-color: #f4f7f9; margin: 0; padding-bottom: 100px; /* Space for sticky footer */ }}
                .container {{ max-width: 1200px; margin: 0 auto; padding: 20px; }}
                .header {{ text-align: center; margin-bottom: 20px; }}
                .header h1 {{ color: #333; }}
                .header p {{ color: #666; font-size: 1.1em; }}
                .days-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 20px; }}
                .day-card {{ background: white; border-radius: 8px; box-shadow: 0 4px 6px rgba(0,0,0,0.05); padding: 20px; border: 1px solid #eef; }}
                .day-card h3 {{ margin-top: 0; border-bottom: 1px solid #eee; padding-bottom: 10px; color: #007bff; font-size: 1.1em; }}
                .day-card h3 span {{ color: #6c757d; font-weight: normal; font-size: 0.9em; }}
                .meal-options {{ display: flex; flex-direction: column; gap: 15px; margin-top: 15px; }}
                .meal-toggle input[type="checkbox"] {{ display: none; }}
                .meal-toggle label {{ display: block; padding: 12px; border-radius: 6px; border: 1px solid #ddd; cursor: pointer; transition: all 0.2s ease; text-align: center; font-weight: 500; }}
                .meal-toggle input[type="checkbox"]:checked + label {{ background-color: #28a745; color: white; border-color: #28a745; box-shadow: 0 2px 5px rgba(40, 167, 69, 0.4); }}
                .sticky-footer {{ position: fixed; bottom: 0; left: 0; width: 100%; background: white; padding: 15px; box-shadow: 0 -2px 10px rgba(0,0,0,0.1); text-align: center; z-index: 100; }}
                .save-btn {{ background-color: #007bff; color: white; padding: 15px 30px; border: none; border-radius: 8px; font-size: 1.1em; font-weight: bold; cursor: pointer; transition: background-color 0.2s; }}
                .save-btn:hover {{ background-color: #0056b3; }}
                .nav-link {{ display: block; text-align: center; margin-top: 20px; font-weight: 500; color: #007bff; }}
            </style>
        </head>
        <body>
            <div class="container">
                <div class="header">
                    <h1>🍳 Interesse em Refeições</h1>
                    <p>Período: {} a {}</p>
                </div>
                <form method="POST" action="/refeicoes/save_all">
                    <div class="days-grid">{}</div>
                    <div class="sticky-footer">
                        <button type="submit" class="save-btn">Guardar Marcações</button>
                    </div>
                </form>
                 <a href="/dashboard" class="nav-link">Voltar ao Dashboard</a>
            </div>
        </body>
        </html>
        "#,
        period.start_date.format("%d/%m/%Y"),
        period.end_date.format("%d/%m/%Y"),
        day_cards_html
    ))
    .into_response()
}