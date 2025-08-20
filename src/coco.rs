// src/escala_admin_handlers.rs

use crate::auth::{self, AppState, User};
use crate::escala::{self, EstadoEscala, EscalaDiaria, Posto, TipoServico, StatusTroca, Troca, Alocacao, Divida, DividasAtivas, Indisponibilidade, Punicao, ConfiguracaoEscala, DetalheServico, TipoTroca};
use crate::escala_pdf; // Importa o novo módulo
use axum::{
    debug_handler,
    extract::{State, Form},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use chrono::{NaiveDate, Duration};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use tokio::fs;
use tower_cookies::Cookies;

// Constantes usadas pelos handlers de admin
const ESTADO_ESCALA_FILE: &str = "data/escala/estado.json";
const POSTOS_FILE: &str = "data/escala/postos.json";
const TROCAS_FILE: &str = "data/escala/trocas.json";
const ESCALA_DATA_DIR: &str = "data/escala";
const USERS_FILE: &str = "users.json";
const DIVIDAS_FILE: &str = "data/escala/dividas.json";
const INDISPONIBILIDADE_FILE: &str = "data/escala/indisponibilidade.json";
const PUNIDOS_FILE: &str = "data/escala/punidos.json";
const CONFIGURACAO_FILE: &str = "data/escala/configuracao.json";


// --- STRUCTS PARA FORMULÁRIOS ---
#[derive(Deserialize)]
pub struct AdicionarIndisponibilidadeForm {
    user_id: String,
    data: NaiveDate,
    motivo: String,
}

#[derive(Deserialize)]
pub struct RemoverIndisponibilidadeForm {
    user_id: String,
    data: NaiveDate,
}

#[derive(Deserialize)]
pub struct AdicionarPunicaoForm {
    user_id: String,
    total_a_cumprir: u32,
}

#[derive(Deserialize)]
pub struct RemoverPunicaoForm {
    user_id: String,
}

/// Verifica se um utilizador está escalado no dia anterior ou seguinte a uma data de serviço.
async fn verificar_risco_fadiga(user_id: &str, data_servico: NaiveDate) -> bool {
    let datas_a_verificar = vec![
        data_servico - Duration::days(1),
        data_servico + Duration::days(1)
    ];

    for data in datas_a_verificar {
        let filename = format!("{}/{}.json", ESCALA_DATA_DIR, data.format("%Y-%m-%d"));
        if let Ok(content) = fs::read_to_string(filename).await {
            if let Ok(escala_diaria) = serde_json::from_str::<EscalaDiaria>(&content) {
                for posto in escala_diaria.escala.values() {
                    for alocacao in posto.values() {
                        if alocacao.user_id == user_id { return true; }
                    }
                }
                for alocacao in &escala_diaria.retem {
                    if alocacao.user_id == user_id { return true; }
                }
            }
        }
    }
    false
}

#[debug_handler]
pub async fn admin_escala_page(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, Html("Acesso negado.")).into_response();
    }
    
    let estado_content = fs::read_to_string(ESTADO_ESCALA_FILE).await.unwrap_or_default();
    let estado: EstadoEscala = serde_json::from_str(&estado_content).unwrap();
    let users_content = fs::read_to_string(USERS_FILE).await.unwrap_or_else(|_| "{}".to_string());
    let users: HashMap<String, User> = serde_json::from_str::<Vec<User>>(&users_content).unwrap_or_default().into_iter().map(|u| (u.id.clone(), u)).collect();
    
    let status_trocas_str = if estado.status_trocas == "Aberto" { "<strong style='color: green;'>Abertas</strong>" } else { "<strong style='color: red;'>Fechadas</strong>" };
    let card_escala_atual_html = format!(r#"..."#); // Omitido por brevidade
    let mut card_geracao_html = String::new();
    let mut card_lancamento_html = String::new();
    if let Some(periodo_seguinte) = &estado.periodo_seguinte {
        card_lancamento_html = format!(r#"..."#); // Omitido por brevidade
        card_geracao_html = r#"..."#.to_string(); // Omitido por brevidade
    } else {
        card_geracao_html = r#"..."#.to_string(); // Omitido por brevidade
    }
    let (status_text_trocas, trade_button_html) = if estado.status_trocas == "Aberto" { ("Aberto".to_string(), r#"..."#.to_string()) } else { ("Fechado".to_string(), r#"..."#.to_string()) };
    let card_gestao_trocas_html = format!(r#"..."#); // Omitido por brevidade

    // --- NOVO CARD PARA PDF ---
    let card_pdf_html = r#"
        <div class="card">
            <h2>Exportar Escala</h2>
            <p>Gere um ficheiro PDF da escala atualmente em vigor para impressão ou arquivo.</p>
            <a href="/admin/escala/pdf" class="btn btn-primary" style="background-color: #6f42c1;">Gerar PDF da Escala Ativa</a>
        </div>
    "#.to_string();
    
    let trocas_pendentes_html = "...".to_string(); // Omitido por brevidade
    let indisponibilidades_html = "...".to_string(); // Omitido por brevidade
    let punicoes_html = "...".to_string(); // Omitido por brevidade
    let user_options_html = "...".to_string(); // Omitido por brevidade
    let postos_html = "...".to_string(); // Omitido por brevidade
    let todos_postos_json = "...".to_string(); // Omitido por brevidade
    let postos_selecionados_json = "...".to_string(); // Omitido por brevidade
    
    Html(format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>...</head>
        <body>
            ...
            <div id="Gestao" class="tabcontent">
                {card_lancamento_html}
                {card_geracao_html}
                {card_gestao_trocas_html}
                {card_pdf_html}
            </div>
            ...
        </body>
        </html>
        "#,
        card_lancamento_html = card_lancamento_html,
        card_geracao_html = card_geracao_html,
        card_gestao_trocas_html = card_gestao_trocas_html,
        card_pdf_html = card_pdf_html,
        // ... resto dos argumentos
    )).into_response()
}

// ... (outros handlers: fechar_trocas, reabrir_trocas, etc.)

// --- NOVO HANDLER PARA GERAR O PDF ---
#[debug_handler]
pub async fn gerar_pdf_escala_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }

    // 1. Carregar dados essenciais
    let user_id = cookies.get("user_id").map(|c| c.value().to_string()).unwrap_or_default();
    let users_content = match fs::read_to_string(USERS_FILE).await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Falha ao ler utilizadores.").into_response(),
    };
    let users: HashMap<String, User> = serde_json::from_str::<Vec<User>>(&users_content).unwrap_or_default()
        .into_iter().map(|u| (u.id.clone(), u)).collect();

    let estado_content = match fs::read_to_string(ESTADO_ESCALA_FILE).await {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Falha ao ler estado da escala.").into_response(),
    };
    let estado: EstadoEscala = serde_json::from_str(&estado_content).unwrap();
    let periodo_ativo = &estado.periodo_atual;

    // 2. Carregar todas as escalas diárias do período ativo
    let mut escalas_map = BTreeMap::new();
    let mut current_date = periodo_ativo.start_date;
    while current_date <= periodo_ativo.end_date {
        let filename = format!("{}/{}.json", ESCALA_DATA_DIR, current_date.format("%Y-%m-%d"));
        if let Ok(content) = fs::read_to_string(&filename).await {
            if let Ok(escala_diaria) = serde_json::from_str::<EscalaDiaria>(&content) {
                escalas_map.insert(current_date, escala_diaria);
            }
        }
        current_date += Duration::days(1);
    }

    // 3. Determinar a assinatura dinâmica
    let user_logado = match users.get(&user_id) {
        Some(u) => u,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "Utilizador logado não encontrado.").into_response(),
    };
    let prioridade_cargos = ["Chefe de Dia", "Polícia", "Escalante", "Admin"];
    let mut cargo_dinamico = "Admin"; // Padrão

    for cargo in prioridade_cargos {
        if auth::has_role(&state, &cookies, cargo).await {
            cargo_dinamico = cargo;
            break;
        }
    }

    // 4. Montar a estrutura de dados para o gerador de PDF
    let pdf_data = escala_pdf::PdfData {
        periodo: periodo_ativo,
        escalas: &escalas_map,
        users: &users,
        info_assinatura_fixa: ("Nome Fixo", "Cargo Fixo"),
        info_assinatura_dinamica: (&user_logado.name, cargo_dinamico),
    };

    // 5. Gerar o PDF
    match escala_pdf::gerar_pdf_da_escala_ativa(pdf_data) {
        Ok(pdf_bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/pdf".parse().unwrap());
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"escala_{}_a_{}.pdf\"", periodo_ativo.start_date, periodo_ativo.end_date)
                    .parse()
                    .unwrap(),
            );
            (headers, pdf_bytes).into_response()
        }
        Err(e) => {
            eprintln!("Erro ao gerar PDF: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao gerar o PDF.").into_response()
        }
    }
}
