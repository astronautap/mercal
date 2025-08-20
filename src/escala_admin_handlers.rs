// src/escala_admin_handlers.rs

use crate::auth::{self, AppState, User};
use crate::escala::{self, EstadoEscala, EscalaDiaria, Posto, TipoServico, StatusTroca, Troca, Alocacao, Divida, DividasAtivas, Indisponibilidade, Punicao, ConfiguracaoEscala, DetalheServico, TipoTroca};
use axum::http::{header, HeaderMap};
use axum::{
    debug_handler,
    extract::{State, Form},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use chrono::{NaiveDate, Duration};
use serde::{Deserialize};
use std::collections::{BTreeMap, HashMap};
use tokio::fs;
use tower_cookies::Cookies;
use crate::escala_pdf;

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

// NOVO STRUCT PARA O FORMULÁRIO DE TROCA OBRIGATÓRIA
#[derive(Deserialize, Debug)]
pub struct TrocaObrigatoriaForm {
    original_service_json: String,
    substitute_user_id: String,
}


/// Verifica se um utilizador está escalado nos dias fornecidos. Retorna (true, "motivo") se encontrar um conflito.
async fn verificar_conflito_escala(user_id: &str, datas_a_verificar: &[(NaiveDate, &'static str)]) -> (bool, &'static str) {
    for (data, motivo) in datas_a_verificar {
        let filename = format!("{}/{}.json", ESCALA_DATA_DIR, data.format("%Y-%m-%d"));
        if let Ok(content) = fs::read_to_string(filename).await {
            if let Ok(escala_diaria) = serde_json::from_str::<EscalaDiaria>(&content) {
                // Verifica na escala normal
                for posto in escala_diaria.escala.values() {
                    for alocacao in posto.values() {
                        if alocacao.user_id == user_id {
                            return (true, motivo);
                        }
                    }
                }
                // Verifica no retém
                for alocacao in &escala_diaria.retem {
                    if alocacao.user_id == user_id {
                        return (true, motivo);
                    }
                }
            }
        }
    }
    (false, "")
}

// --- HANDLERS ---

// NOVO HANDLER PARA TROCA OBRIGATÓRIA
#[debug_handler]
pub async fn troca_obrigatoria_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<TrocaObrigatoriaForm>,
) -> impl IntoResponse {
    // 1. Validação de permissão
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, Html("Acesso negado.")).into_response();
    }

    // 2. Deserializar os detalhes do serviço original
    let original_service: DetalheServico = match serde_json::from_str(&form.original_service_json) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, Html("Dados do serviço original inválidos.")).into_response(),
    };

    // 3. Carregar todos os utilizadores para encontrar o substituto
    let users: HashMap<String, User> = match fs::read_to_string(USERS_FILE).await {
        Ok(c) => serde_json::from_str::<Vec<User>>(&c).unwrap_or_default().into_iter().map(|u| (u.id.clone(), u)).collect(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Html("Falha ao carregar utilizadores.")).into_response(),
    };

    let substitute_user = match users.get(&form.substitute_user_id) {
        Some(u) => u,
        None => return (StatusCode::BAD_REQUEST, Html("ID do substituto não encontrado.")).into_response(),
    };

    // 4. Verificar disponibilidade e risco de fadiga para o substituto
    let datas_verificacao = vec![
        (original_service.data, "já está de serviço no mesmo dia"),
        (original_service.data - Duration::days(1), "está de serviço no dia anterior (risco de fadiga)"),
        (original_service.data + Duration::days(1), "está de serviço no dia seguinte (risco de fadiga)"),
    ];
    let (conflito, motivo) = verificar_conflito_escala(&substitute_user.id, &datas_verificacao).await;
    if conflito {
        let error_message = format!("<h1>Erro: Conflito de Escala!</h1><p>O substituto selecionado {}. A troca não foi efetuada.</p><a href='/escala'>Voltar</a>", motivo);
        return (StatusCode::CONFLICT, Html(error_message)).into_response();
    }

    // 5. Carregar e modificar a escala diária
    let filename = format!("{}/{}.json", ESCALA_DATA_DIR, original_service.data.format("%Y-%m-%d"));
    let mut escala_diaria: EscalaDiaria = match fs::read_to_string(&filename).await {
        Ok(c) => serde_json::from_str(&c).unwrap(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Html("Falha ao carregar a escala do dia.")).into_response(),
    };

    let nova_alocacao = Alocacao {
        user_id: substitute_user.id.clone(),
        nome: format!("{} (TO)", substitute_user.name), // Adiciona a marcação (TO)
        punicao: false,
    };

    let mut sucesso = false;
    if original_service.posto == "RETEM" {
        if let Some(aloc) = escala_diaria.retem.iter_mut().find(|a| a.user_id == original_service.user_id) {
            *aloc = nova_alocacao;
            sucesso = true;
        }
    } else {
        if let Some(horarios) = escala_diaria.escala.get_mut(&original_service.posto) {
            if let Some(aloc) = horarios.get_mut(&original_service.horario) {
                if aloc.user_id == original_service.user_id {
                    *aloc = nova_alocacao;
                    sucesso = true;
                }
            }
        }
    }

    if !sucesso {
        return (StatusCode::INTERNAL_SERVER_ERROR, Html("Não foi possível encontrar o serviço para substituir.")).into_response();
    }

    // 6. Salvar a escala diária modificada
    if let Err(_) = fs::write(&filename, serde_json::to_string_pretty(&escala_diaria).unwrap()).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Html("Falha ao salvar a escala modificada.")).into_response();
    }

    // 7. Gerar a dívida para o utilizador substituído
    let mut dividas: DividasAtivas = fs::read_to_string(DIVIDAS_FILE).await.ok().and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default();
    let divida = Divida {
        credor: substitute_user.id.clone(), // O substituto é o credor
        tipo_divida: escala_diaria.tipo_dia,
    };
    dividas.entry(original_service.user_id).or_default().push(divida); // A dívida é do utilizador original

    if let Err(_) = fs::write(DIVIDAS_FILE, serde_json::to_string_pretty(&dividas).unwrap()).await {
        eprintln!("AVISO: Falha ao salvar a dívida da troca obrigatória.");
    }

    // 8. Redirecionar de volta para a página da escala
    Redirect::to("/escala").into_response()
}

#[debug_handler]
pub async fn admin_escala_page(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, Html("Acesso negado.")).into_response();
    }
    
    // --- 1. Carregamento de todos os dados necessários ---
    let estado_content = fs::read_to_string(ESTADO_ESCALA_FILE).await.unwrap_or_default();
    let estado: EstadoEscala = serde_json::from_str(&estado_content).unwrap();

    let users_content = fs::read_to_string(USERS_FILE).await.unwrap_or_else(|_| "{}".to_string());
    let users: HashMap<String, User> = serde_json::from_str::<Vec<User>>(&users_content).unwrap_or_default().into_iter().map(|u| (u.id.clone(), u)).collect();
    
    // --- 2. Geração de todos os snippets HTML ---

    let status_trocas_str = if estado.status_trocas == "Aberto" { 
        "<strong style='color: green;'>Abertas</strong>"
    } else { 
        "<strong style='color: red;'>Fechadas</strong>"
    };
    let card_escala_atual_html = format!(
        r#"<div class="card" style="border-left: 4px solid #28a745;">
               <h2>Escala em Vigor</h2>
               <p>O período da escala atual é de <strong>{}</strong> a <strong>{}</strong>.</p>
               <p><strong>Status das Trocas:</strong> {}</p>
           </div>"#,
        estado.periodo_atual.start_date.format("%d/%m/%Y"),
        estado.periodo_atual.end_date.format("%d/%m/%Y"),
        status_trocas_str
    );

    let card_geracao_html: String;
    let mut card_lancamento_html = String::new();
    if let Some(periodo_seguinte) = &estado.periodo_seguinte {
        card_lancamento_html = format!(
            r#"<div class="card" style="border-left: 4px solid #17a2b8;">
                   <h2>Próxima Escala Pendente</h2>
                   <p>Uma próxima escala já foi gerada para o período de <strong>{}</strong> a <strong>{}</strong>.</p>
                   <form action="/admin/escala/lancar" method="post" onsubmit="return confirm('Tem a certeza que deseja tornar esta a escala atual? Esta ação não pode ser desfeita.');">
                       <button type="submit" class="btn btn-primary">Lançar e Tornar Atual</button>
                   </form>
               </div>"#,
            periodo_seguinte.start_date.format("%d/%m/%Y"),
            periodo_seguinte.end_date.format("%d/%m/%Y")
        );
        card_geracao_html = r#"<div class="card"><h2>Gerar Nova Escala</h2><p style="color: #6c757d;">Para gerar uma nova escala, é preciso primeiro lançar a escala pendente acima.</p></div>"#.to_string();
    } else {
        card_geracao_html = r#"
            <div class="card">
                <h2>Gerar Nova Escala</h2>
                <form id="form-gerar-escala" action="/admin/escala/gerar" method="post">
                    <p><label for="start_date">Data de Início:</label><input type="date" id="start_date" name="start_date" required></p>
                    <p><label for="end_date">Data de Fim:</label><input type="date" id="end_date" name="end_date" required></p>
                    <h3>Definir Tipo de Rotina para cada Dia</h3>
                    <div id="dias-da-semana-container" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 15px;">
                        <p style="color: #666; grid-column: 1 / -1;"><em>Selecione as datas de início e fim para configurar os dias.</em></p>
                    </div>
                    <br>
                    <button type="submit" class="btn btn-primary">Gerar Escala</button>
                </form>
            </div>"#.to_string();
    }

    let (status_text_trocas, trade_button_html) = if estado.status_trocas == "Aberto" {
        ("Aberto".to_string(), r#"<form action="/admin/escala/fechar_trocas" method="post"><button type="submit" class="btn btn-danger">Fechar Período de Trocas</button></form>"#.to_string())
    } else {
        ("Fechado".to_string(), r#"<form action="/admin/escala/reabrir_trocas" method="post"><button type="submit" class="btn btn-success">Reabrir Período de Trocas</button></form>"#.to_string())
    };
    let card_gestao_trocas_html = format!(
        r#"<div class="card"><h2>Gestão do Período de Trocas</h2><p>Estado atual do período de trocas (para a Próxima Escala): <strong>{}</strong></p>{}</div>"#,
        status_text_trocas, trade_button_html
    );

    let first_day_of_scale_file = format!("{}/{}.json", ESCALA_DATA_DIR, estado.periodo_atual.start_date.format("%Y-%m-%d"));
    let scale_exists = fs::try_exists(&first_day_of_scale_file).await.unwrap_or(false);

    let card_pdf_html = if scale_exists {
        r#"
        <div class="card">
            <h2>Exportar Escala</h2>
            <p>Gere um ficheiro PDF da escala atualmente em vigor para impressão ou arquivo.</p>
            <a href="/admin/escala/pdf" class="btn btn-primary" style="background-color: #6f42c1;">Gerar PDF da Escala Ativa</a>
        </div>
        "#.to_string()
    } else {
        r#"
        <div class="card">
            <h2>Exportar Escala</h2>
            <p style="color: #6c757d;">Não há uma escala ativa gerada para o período atual. Gere e lance uma nova escala para poder exportar o PDF.</p>
            <button class="btn btn-primary" style="background-color: #6c757d; cursor: not-allowed;" disabled>Gerar PDF da Escala Ativa</button>
        </div>
        "#.to_string()
    };

    let trocas_content = fs::read_to_string(TROCAS_FILE).await.unwrap_or_else(|_| "[]".to_string());
    let todas_as_trocas: Vec<Troca> = serde_json::from_str(&trocas_content).unwrap_or_default();
    let mut trocas_pendentes_html = String::new();
    if !todas_as_trocas.iter().any(|t| t.status == StatusTroca::PendenteAdmin) {
        trocas_pendentes_html = "<p>Não há pedidos de troca pendentes de aprovação.</p>".to_string();
    } else {
        for troca in todas_as_trocas.iter().filter(|t| t.status == StatusTroca::PendenteAdmin) {
            
            let datas_verificacao_req = vec![(troca.alvo.data - Duration::days(1), ""), (troca.alvo.data + Duration::days(1), "")];
            let (fadiga_req, _) = verificar_conflito_escala(&troca.requerente.user_id, &datas_verificacao_req).await;
            
            let mut fadiga_alvo = false;
            if troca.tipo == escala::TipoTroca::Permuta {
                let datas_verificacao_alvo = vec![(troca.requerente.data - Duration::days(1), ""), (troca.requerente.data + Duration::days(1), "")];
                (fadiga_alvo, _) = verificar_conflito_escala(&troca.alvo.user_id, &datas_verificacao_alvo).await;
            }

            let warning_html = if fadiga_req || fadiga_alvo {
                "<p style='color: red; font-weight: bold;'>⚠️ Aviso: A aprovação desta troca pode resultar em fadiga.</p>".to_string()
            } else { "".to_string() };

            trocas_pendentes_html.push_str(&format!(
                r#"<div class="trade-request" style="border-top: 1px solid #eee; padding-top: 10px; margin-top: 10px;">
                    <p><strong>Pedido de {tipo:?}:</strong> {req_id} ({req_posto} em {req_data}) quer trocar com {alvo_id} ({alvo_posto} em {alvo_data}).</p>
                    <p><i>Motivo: {motivo}</i></p>
                    {warning_html}
                    <form action="/admin/escala/aprovar_troca" method="post" style="display: inline-block; margin-right: 5px;">
                        <input type="hidden" name="troca_id" value="{id}"><input type="hidden" name="acao" value="aprovar">
                        <button type="submit" class="btn btn-success">Aprovar</button>
                    </form>
                    <form action="/admin/escala/aprovar_troca" method="post" style="display: inline-block;">
                        <input type="hidden" name="troca_id" value="{id}"><input type="hidden" name="acao" value="recusar">
                        <button type="submit" class="btn btn-danger">Recusar</button>
                    </form>
                </div>"#,
                tipo = troca.tipo, req_id = troca.requerente.user_id, req_posto = troca.requerente.posto, req_data = troca.requerente.data.format("%d/%m"),
                alvo_id = troca.alvo.user_id, alvo_posto = troca.alvo.posto, alvo_data = troca.alvo.data.format("%d/%m"),
                motivo = troca.motivo, id = troca.id, warning_html = warning_html
            ));
        }
    }

    let indisponibilidades_content = fs::read_to_string(INDISPONIBILIDADE_FILE).await.unwrap_or_else(|_| "[]".to_string());
    let indisponibilidades: Vec<Indisponibilidade> = serde_json::from_str(&indisponibilidades_content).unwrap_or_default();
    let mut indisponibilidades_html = String::new();
    if indisponibilidades.is_empty() {
        indisponibilidades_html.push_str("<p>Não há utilizadores marcados como indisponíveis.</p>");
    } else {
        indisponibilidades_html.push_str("<table><thead><tr><th>ID</th><th>Nome</th><th>Data</th><th>Motivo</th><th>Ação</th></tr></thead><tbody>");
        for ind in &indisponibilidades {
            let user_name = users.get(&ind.user_id).map_or("Desconhecido", |u| u.name.as_str());
            indisponibilidades_html.push_str(&format!(
                r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>
                   <form action="/admin/escala/indisponibilidade/remover" method="post" style="margin:0;">
                   <input type="hidden" name="user_id" value="{}"><input type="hidden" name="data" value="{}">
                   <button type="submit" class="btn btn-danger btn-sm">Remover</button></form></td></tr>"#,
                ind.user_id, user_name, ind.data.format("%d/%m/%Y"), ind.motivo, ind.user_id, ind.data
            ));
        }
        indisponibilidades_html.push_str("</tbody></table>");
    }

    let punidos_content = fs::read_to_string(PUNIDOS_FILE).await.unwrap_or_else(|_| "[]".to_string());
    let punicoes: Vec<Punicao> = serde_json::from_str(&punidos_content).unwrap_or_default();
    let mut punicoes_html = String::new();
    if punicoes.is_empty() {
        punicoes_html.push_str("<p>Não há utilizadores com punições ativas.</p>");
    } else {
        punicoes_html.push_str("<table><thead><tr><th>ID</th><th>Nome</th><th>Progresso</th><th>Ação</th></tr></thead><tbody>");
        for punicao in &punicoes {
            let user_name = users.get(&punicao.user_id).map_or("Desconhecido", |u| u.name.as_str());
            punicoes_html.push_str(&format!(
                r#"<tr><td>{}</td><td>{}</td><td>{}/{}</td><td>
                   <form action="/admin/escala/punicao/remover" method="post" style="margin:0;">
                   <input type="hidden" name="user_id" value="{}"><button type="submit" class="btn btn-danger btn-sm">Remover</button></form></td></tr>"#,
                punicao.user_id, user_name, punicao.ja_cumpridos, punicao.total_a_cumprir, punicao.user_id
            ));
        }
        punicoes_html.push_str("</tbody></table>");
    }

    let mut user_options_html = String::new();
    for user in users.values() {
        user_options_html.push_str(&format!("<option value='{}'>{} - {}</option>", user.id, user.id, user.name));
    }
    
    let config_content = fs::read_to_string(CONFIGURACAO_FILE).await.unwrap_or_else(|_| r#"{ "postos_punicao": [] }"#.to_string());
    let config: ConfiguracaoEscala = serde_json::from_str(&config_content).unwrap_or_default();
    let postos_content = fs::read_to_string(POSTOS_FILE).await.unwrap_or_else(|_| "[]".to_string());
    let todos_postos: Vec<Posto> = serde_json::from_str(&postos_content).unwrap_or_default();
    
    let todos_postos_nomes: Vec<&str> = todos_postos.iter().map(|p| p.nome.as_str()).collect();
    let todos_postos_json = serde_json::to_string(&todos_postos_nomes).unwrap_or_else(|_| "[]".to_string());
    let postos_selecionados_json = serde_json::to_string(&config.postos_punicao).unwrap_or_else(|_| "[]".to_string());

    let mut postos_html = String::new();
    if todos_postos.is_empty() {
        postos_html.push_str("<p>Nenhum posto configurado.</p>");
    } else {
        postos_html.push_str("<ul>");
        for posto in &todos_postos {
            postos_html.push_str(&format!("<li>{}</li>", posto.nome));
        }
        postos_html.push_str("</ul>");
    }

    Html(format!(
        r#"
        <!DOCTYPE html>
        <html lang="pt-BR">
        <head>
            <title>Admin - Gestão de Escalas</title>
            <meta charset="UTF-8">
            <style>
                body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; max-width: 900px; margin: 40px auto; padding: 20px; background-color: #f4f7f9; color: #333; }}
                .card {{ background: white; border: 1px solid #e0e0e0; padding: 25px; border-radius: 8px; margin-bottom: 25px; box-shadow: 0 4px 6px rgba(0,0,0,0.05); }}
                h1, h2 {{ color: #0056b3; }}
                .btn {{ display: inline-block; padding: 10px 15px; border-radius: 6px; text-decoration: none; color: white; border: none; cursor: pointer; font-size: 14px; margin-right: 10px; }}
                .btn-primary {{ background-color: #007bff; }} .btn-danger {{ background-color: #dc3545; }} .btn-success {{ background-color: #28a745; }}
                .tab {{ overflow: hidden; border-bottom: 1px solid #ccc; margin-bottom: 20px; }}
                .tab button {{ background-color: inherit; float: left; border: none; outline: none; cursor: pointer; padding: 14px 16px; transition: 0.3s; font-size: 17px; border-radius: 6px 6px 0 0; }}
                .tab button.active {{ background-color: #0056b3; color: white; }}
                .tabcontent {{ display: none; }}
                table {{ width: 100%; border-collapse: collapse; }} th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }} th {{ background-color: #f2f2f2; }}
                .tag-selector-wrapper {{ position: relative; }}
                .tag-container {{ display: flex; flex-wrap: wrap; gap: 6px; padding: 8px; border: 1px solid #ccc; border-radius: 4px; margin-bottom: 5px; min-height: 38px; }}
                .tag {{ display: flex; align-items: center; background-color: #007bff; color: white; padding: 4px 8px; border-radius: 4px; font-size: 14px; }}
                .tag-remove {{ margin-left: 8px; cursor: pointer; font-weight: bold; }}
                .dropdown-content {{ display: none; position: absolute; background-color: #f9f9f9; width: 100%; box-shadow: 0px 8px 16px 0px rgba(0,0,0,0.2); z-index: 1; max-height: 200px; overflow-y: auto; border: 1px solid #ddd; border-top: none; }}
                .dropdown-content div {{ color: black; padding: 12px 16px; text-decoration: none; display: block; cursor: pointer; }}
                .dropdown-content div:hover {{ background-color: #f1f1f1; }}
            </style>
        </head>
        <body>
            <h1>Painel de Controlo de Escalas</h1>
            {card_escala_atual_html}
            <div class="tab">
                <button class="tablink" onclick="openTab(event, 'Gestao')" id="defaultOpen">Gestão</button>
                <button class="tablink" onclick="openTab(event, 'Aprovacao')">Aprovações</button>
                <button class="tablink" onclick="openTab(event, 'Indisponibilidade')">Indisponibilidades</button>
                <button class="tablink" onclick="openTab(event, 'Punicao')">Punições</button>
                <button class="tablink" onclick="openTab(event, 'Config')">Outras Configurações</button>
            </div>
            <div id="Gestao" class="tabcontent">{card_lancamento_html}{card_geracao_html}{card_gestao_trocas_html}{card_pdf_html}</div>
            <div id="Aprovacao" class="tabcontent"><div class="card"><h2>Aprovação de Trocas</h2>{trocas_pendentes_html}</div></div>
            <div id="Indisponibilidade" class="tabcontent">
                <div class="card"><h2>Utilizadores Indisponíveis</h2>{indisponibilidades_html}</div>
                <div class="card"><h2>Adicionar Indisponibilidade</h2>
                    <form action="/admin/escala/indisponibilidade/adicionar" method="post">
                        <p><label>Utilizador:</label><input list="user-list" name="user_id" required><datalist id="user-list">{user_options_html}</datalist></p>
                        <p><label>Data:</label><input type="date" name="data" required></p>
                        <p><label>Motivo:</label><input type="text" name="motivo" required></p>
                        <button type="submit" class="btn btn-primary">Adicionar</button>
                    </form>
                </div>
            </div>
            <div id="Punicao" class="tabcontent">
                <div class="card"><h2>Punições Ativas</h2>{punicoes_html}</div>
                <div class="card"><h2>Adicionar Punição</h2>
                    <form action="/admin/escala/punicao/adicionar" method="post">
                        <p><label>Utilizador:</label><input list="user-list" name="user_id" required><datalist id="user-list">{user_options_html}</datalist></p>
                        <p><label>Nº de Serviços:</label><input type="number" name="total_a_cumprir" min="1" required></p>
                        <button type="submit" class="btn btn-primary">Adicionar</button>
                    </form>
                </div>
                <div class="card"><h2>Configurar Postos de Punição</h2>
                    <p>Selecione os postos que podem ser preenchidos por utilizadores punidos.</p>
                    <form action="/admin/escala/configuracao/salvar" method="post" id="punicao-form">
                        <div class="tag-selector-wrapper">
                            <div class="tag-container" id="tag-container"></div>
                            <input type="text" id="tag-search-input" placeholder="Pesquisar e adicionar posto..." autocomplete="off">
                            <div class="dropdown-content" id="dropdown-content"></div>
                        </div>
                        <br><button type="submit" class="btn btn-primary">Salvar Configuração</button>
                    </form>
                </div>
            </div>
            <div id="Config" class="tabcontent"><div class="card"><h2>Postos Configurados no Sistema</h2>{postos_html}</div></div>
            <a href="/dashboard">← Voltar ao Dashboard</a>
            
            <script>
                function openTab(evt, tabName) {{
                    document.querySelectorAll(".tabcontent").forEach(tc => tc.style.display = "none");
                    document.querySelectorAll(".tablink").forEach(tl => tl.classList.remove("active"));
                    document.getElementById(tabName).style.display = "block";
                    evt.currentTarget.classList.add("active");
                }}
                document.getElementById("defaultOpen").click();

                const formGerar = document.getElementById('form-gerar-escala');
                if (formGerar) {{
                    const periodoAtualStart = new Date('{periodo_atual_start}T00:00:00');
                    const periodoAtualEnd = new Date('{periodo_atual_end}T00:00:00');
                    const startDateInput = document.getElementById('start_date');
                    const endDateInput = document.getElementById('end_date');
                    const container = document.getElementById('dias-da-semana-container');

                    formGerar.addEventListener('submit', function(event) {{
                        if (!startDateInput.value || !endDateInput.value) return;
                        const newStart = new Date(startDateInput.value + 'T00:00:00');
                        const newEnd = new Date(endDateInput.value + 'T00:00:00');
                        if (newStart <= periodoAtualEnd && newEnd >= periodoAtualStart) {{
                            if (!confirm('AVISO: O período selecionado sobrepõe-se à escala atual em vigor.\\n\\nSe continuar, os ficheiros da escala para os dias sobrepostos serão regenerados.\\n\\nDeseja continuar?')) {{
                                event.preventDefault();
                            }}
                        }}
                    }});
                    
                    function updateDaySelectors() {{
                        const start = startDateInput.value;
                        const end = endDateInput.value;
                        if (!start || !end || new Date(end) < new Date(start)) {{
                            container.innerHTML = '<p style="color: #666;"><em>Selecione um período válido.</em></p>';
                            return;
                        }}
                        container.innerHTML = '';
                        let currentDate = new Date(start + 'T00:00:00');
                        const endDate = new Date(end + 'T00:00:00');
                        const weekdays = ['Domingo', 'Segunda', 'Terça', 'Quarta', 'Quinta', 'Sexta', 'Sábado'];
                        while(currentDate <= endDate) {{
                            const dateString = currentDate.toISOString().split('T')[0];
                            const dayName = weekdays[currentDate.getUTCDay()];
                            const div = document.createElement('div');
                            div.innerHTML = `
                                <p><strong>${{dayName}}</strong> (${{currentDate.toLocaleDateString('pt-BR', {{timeZone: 'UTC'}})}})</p>
                                <div>
                                    <input type="radio" id="rn-${{dateString}}" name="tipo_dia_${{dateString}}" value="RN" checked><label for="rn-${{dateString}}">RN</label>
                                    <input type="radio" id="rd-${{dateString}}" name="tipo_dia_${{dateString}}" value="RD"><label for="rd-${{dateString}}">RD</label>
                                    <input type="radio" id="udrd-${{dateString}}" name="tipo_dia_${{dateString}}" value="UDRD"><label for="udrd-${{dateString}}">UDRD</label>
                                    <input type="radio" id="er-${{dateString}}" name="tipo_dia_${{dateString}}" value="ER"><label for="er-${{dateString}}">ER</label>
                                </div>`;
                            container.appendChild(div);
                            currentDate.setUTCDate(currentDate.getUTCDate() + 1);
                        }}
                    }}
                    startDateInput.addEventListener('change', updateDaySelectors);
                    endDateInput.addEventListener('change', updateDaySelectors);
                }}

                const allPosts = {todos_postos_json};
                const selectedPosts = new Set({postos_selecionados_json});
                const searchInput = document.getElementById('tag-search-input');
                const dropdown = document.getElementById('dropdown-content');
                const tagContainer = document.getElementById('tag-container');
                const punicaoForm = document.getElementById('punicao-form');
                
                function render() {{
                    tagContainer.innerHTML = '';
                    Array.from(punicaoForm.querySelectorAll('input[name="postos"]')).forEach(input => input.remove());
                    selectedPosts.forEach(posto => {{
                        const tagEl = document.createElement('div');
                        tagEl.className = 'tag';
                        tagEl.textContent = posto;
                        const removeEl = document.createElement('span');
                        removeEl.className = 'tag-remove';
                        removeEl.innerHTML = '&times;';
                        removeEl.onclick = (e) => {{ e.stopPropagation(); removePost(posto); }};
                        tagEl.appendChild(removeEl);
                        tagContainer.appendChild(tagEl);
                        const hiddenInput = document.createElement('input');
                        hiddenInput.type = 'hidden';
                        hiddenInput.name = 'postos';
                        hiddenInput.value = posto;
                        punicaoForm.appendChild(hiddenInput);
                    }});
                }}

                function addPost(posto) {{ selectedPosts.add(posto); searchInput.value = ''; hideDropdown(); render(); }}
                function removePost(posto) {{ selectedPosts.delete(posto); render(); }}

                function updateDropdown() {{
                    const filter = searchInput.value.toLowerCase();
                    dropdown.innerHTML = '';
                    const available = allPosts.filter(p => !selectedPosts.has(p) && (filter === '' || p.toLowerCase().includes(filter)));
                    if (available.length > 0 && document.activeElement === searchInput) {{
                        available.forEach(posto => {{
                            const itemEl = document.createElement('div');
                            itemEl.textContent = posto;
                            itemEl.onclick = () => addPost(posto);
                            dropdown.appendChild(itemEl);
                        }});
                        dropdown.style.display = 'block';
                    }} else {{ hideDropdown(); }}
                }}

                function hideDropdown() {{ dropdown.style.display = 'none'; }}
                
                searchInput.addEventListener('keyup', updateDropdown);
                searchInput.addEventListener('focus', updateDropdown);
                document.addEventListener('click', (e) => {{ if (!e.target.closest('.tag-selector-wrapper')) {{ hideDropdown(); }} }});
                
                render();
            </script>
        </body>
        </html>
        "#,
        card_escala_atual_html = card_escala_atual_html,
        card_lancamento_html = card_lancamento_html,
        card_geracao_html = card_geracao_html,
        card_gestao_trocas_html = card_gestao_trocas_html,
        trocas_pendentes_html = trocas_pendentes_html,
        indisponibilidades_html = indisponibilidades_html,
        punicoes_html = punicoes_html,
        user_options_html = user_options_html,
        postos_html = postos_html,
        todos_postos_json = todos_postos_json,
        postos_selecionados_json = postos_selecionados_json,
        periodo_atual_start = estado.periodo_atual.start_date.format("%Y-%m-%d"),
        periodo_atual_end = estado.periodo_atual.end_date.format("%Y-%m-%d")
    )).into_response()
}

#[debug_handler]
pub async fn fechar_trocas_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }
    if let Ok(content) = fs::read_to_string(ESTADO_ESCALA_FILE).await {
        if let Ok(mut estado) = serde_json::from_str::<EstadoEscala>(&content) {
            estado.status_trocas = "Fechado".to_string();
            if let Ok(json) = serde_json::to_string_pretty(&estado) {
                let _ = fs::write(ESTADO_ESCALA_FILE, json).await;
            }
        }
    }
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn reabrir_trocas_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }
    if let Ok(content) = fs::read_to_string(ESTADO_ESCALA_FILE).await {
        if let Ok(mut estado) = serde_json::from_str::<EstadoEscala>(&content) {
            estado.status_trocas = "Aberto".to_string();
            if let Ok(json) = serde_json::to_string_pretty(&estado) {
                let _ = fs::write(ESTADO_ESCALA_FILE, json).await;
            }
        }
    }
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn aprovar_troca_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form_data): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }

    let troca_id = form_data.get("troca_id").unwrap().to_string();
    let acao = form_data.get("acao").unwrap();

    let mut trocas: Vec<Troca> = match fs::read_to_string(TROCAS_FILE).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => return Redirect::to("/admin/escala"),
    };
    
    let troca_index = match trocas.iter().position(|t| t.id == troca_id) {
        Some(index) => index,
        None => return Redirect::to("/admin/escala"),
    };

    if trocas[troca_index].status != StatusTroca::PendenteAdmin {
        return Redirect::to("/admin/escala");
    }

    if acao == "recusar" {
        trocas[troca_index].status = StatusTroca::Recusada;
    } else {
        trocas[troca_index].status = StatusTroca::Aprovada;
        let troca = trocas[troca_index].clone();

        let users_content = fs::read_to_string(USERS_FILE).await.unwrap_or_else(|_| "[]".to_string());
        let users: HashMap<String, User> = serde_json::from_str::<Vec<User>>(&users_content)
            .unwrap_or_default().into_iter().map(|u| (u.id.clone(), u)).collect();

        if troca.tipo == TipoTroca::Permuta {
            let req_details = &troca.requerente;
            let alvo_details = &troca.alvo;
            
            let requester_user = users.get(&req_details.user_id).unwrap();
            let target_user = users.get(&alvo_details.user_id).unwrap();

            let new_alocacao_para_slot_requerente = Alocacao { user_id: target_user.id.clone(), nome: format!("{} (TR)", target_user.name), punicao: false };
            let new_alocacao_para_slot_alvo = Alocacao { user_id: requester_user.id.clone(), nome: format!("{} (TR)", requester_user.name), punicao: false };

            let req_filename = format!("{}/{}.json", ESCALA_DATA_DIR, req_details.data.format("%Y-%m-%d"));
            let mut escala_req: EscalaDiaria = serde_json::from_str(&fs::read_to_string(&req_filename).await.unwrap()).unwrap();
            
            if req_details.posto == "RETEM" {
                if let Some(aloc) = escala_req.retem.iter_mut().find(|a| a.user_id == req_details.user_id) { *aloc = new_alocacao_para_slot_requerente.clone(); }
            } else {
                if let Some(aloc) = escala_req.escala.get_mut(&req_details.posto).and_then(|h| h.get_mut(&req_details.horario)) { *aloc = new_alocacao_para_slot_requerente; }
            }

            let alvo_filename = format!("{}/{}.json", ESCALA_DATA_DIR, alvo_details.data.format("%Y-%m-%d"));
            if req_filename == alvo_filename {
                if alvo_details.posto == "RETEM" {
                    if let Some(aloc) = escala_req.retem.iter_mut().find(|a| a.user_id == alvo_details.user_id) { *aloc = new_alocacao_para_slot_alvo; }
                } else {
                    if let Some(aloc) = escala_req.escala.get_mut(&alvo_details.posto).and_then(|h| h.get_mut(&alvo_details.horario)) { *aloc = new_alocacao_para_slot_alvo; }
                }
                fs::write(&req_filename, serde_json::to_string_pretty(&escala_req).unwrap()).await.unwrap();
            } else {
                fs::write(&req_filename, serde_json::to_string_pretty(&escala_req).unwrap()).await.unwrap();
                let mut escala_alvo: EscalaDiaria = serde_json::from_str(&fs::read_to_string(&alvo_filename).await.unwrap()).unwrap();
                if alvo_details.posto == "RETEM" {
                     if let Some(aloc) = escala_alvo.retem.iter_mut().find(|a| a.user_id == alvo_details.user_id) { *aloc = new_alocacao_para_slot_alvo; }
                } else {
                    if let Some(aloc) = escala_alvo.escala.get_mut(&alvo_details.posto).and_then(|h| h.get_mut(&alvo_details.horario)) { *aloc = new_alocacao_para_slot_alvo; }
                }
                fs::write(&alvo_filename, serde_json::to_string_pretty(&escala_alvo).unwrap()).await.unwrap();
            }
        } else { // Cobertura
            let filename = format!("{}/{}.json", ESCALA_DATA_DIR, troca.alvo.data.format("%Y-%m-%d"));
            if let Ok(content) = fs::read_to_string(&filename).await {
                let mut escala_diaria: EscalaDiaria = serde_json::from_str(&content).unwrap();
                let requester_name = users.get(&troca.requerente.user_id).map_or("N/A", |u| &u.name);

                if troca.alvo.posto == "RETEM" {
                    if let Some(aloc) = escala_diaria.retem.iter_mut().find(|a| a.user_id == troca.alvo.user_id) {
                        aloc.user_id = troca.requerente.user_id.clone();
                        aloc.nome = format!("{} (TR)", requester_name);
                        aloc.punicao = false;
                    }
                } else {
                    if let Some(aloc) = escala_diaria.escala.get_mut(&troca.alvo.posto).and_then(|h| h.get_mut(&troca.alvo.horario)) {
                        aloc.user_id = troca.requerente.user_id.clone();
                        aloc.nome = format!("{} (TR)", requester_name);
                        aloc.punicao = false;
                    }
                }
                
                fs::write(&filename, serde_json::to_string_pretty(&escala_diaria).unwrap()).await.unwrap();

                let mut dividas: DividasAtivas = fs::read_to_string(DIVIDAS_FILE).await.ok().and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default();
                let divida = Divida {
                    credor: troca.requerente.user_id.clone(),
                    tipo_divida: escala_diaria.tipo_dia,
                };
                dividas.entry(troca.alvo.user_id.clone()).or_default().push(divida);
                fs::write(DIVIDAS_FILE, serde_json::to_string_pretty(&dividas).unwrap()).await.unwrap();
            }
        }
    }

    if let Ok(json) = serde_json::to_string_pretty(&trocas) {
        let _ = fs::write(TROCAS_FILE, json).await;
    }

    Redirect::to("/admin/escala")
}


#[debug_handler]
pub async fn gerar_escala_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form_data): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, Html("Acesso negado.")).into_response();
    }

    let start_date_str = form_data.get("start_date").cloned().unwrap_or_default();
    let end_date_str = form_data.get("end_date").cloned().unwrap_or_default();

    let start_date = match NaiveDate::parse_from_str(&start_date_str, "%Y-%m-%d") {
        Ok(date) => date,
        Err(_) => return (StatusCode::BAD_REQUEST, Html("Data de início inválida.")).into_response(),
    };
    let end_date = match NaiveDate::parse_from_str(&end_date_str, "%Y-%m-%d") {
        Ok(date) => date,
        Err(_) => return (StatusCode::BAD_REQUEST, Html("Data de fim inválida.")).into_response(),
    };

    if start_date > end_date {
        return (StatusCode::BAD_REQUEST, Html("A data de início não pode ser posterior à data de fim.")).into_response();
    }

    let mut dias_da_escala: HashMap<NaiveDate, TipoServico> = HashMap::new();
    let mut current_date = start_date;

    while current_date <= end_date {
        let date_key = format!("tipo_dia_{}", current_date.format("%Y-%m-%d"));
        if let Some(tipo_str) = form_data.get(&date_key) {
            let tipo_servico = match tipo_str.as_str() {
                "RN" => TipoServico::RN,
                "RD" => TipoServico::RD,
                "UDRD" => TipoServico::UDRD,
                "ER" => TipoServico::ER,
                _ => TipoServico::RN,
            };
            dias_da_escala.insert(current_date, tipo_servico);
        }
        current_date = current_date.succ_opt().unwrap();
    }
    
    match escala::gerar_nova_escala(dias_da_escala).await {
        Ok(_) => println!("✅ Ficheiros de escala diários gerados com sucesso para o próximo período!"),
        Err(e) => {
            eprintln!("🔥 Erro ao gerar escala: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Html(format!("Erro ao gerar escala: {}", e))).into_response();
        }
    }

    println!("A atualizar o período seguinte em estado.json...");
    if let Ok(estado_content) = fs::read_to_string(ESTADO_ESCALA_FILE).await {
        if let Ok(mut estado) = serde_json::from_str::<EstadoEscala>(&estado_content) {
            estado.periodo_seguinte = Some(escala::Periodo {
                start_date,
                end_date,
            });
            
            if let Ok(json_estado) = serde_json::to_string_pretty(&estado) {
                if let Err(e) = fs::write(ESTADO_ESCALA_FILE, json_estado).await {
                    eprintln!("🔥 Falha ao atualizar estado.json: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, Html("Falha ao atualizar o estado do período.")).into_response();
                }
            }
        }
    }

    Redirect::to("/admin/escala").into_response()
}

#[debug_handler]
pub async fn lancar_escala_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }
    
    if let Ok(content) = fs::read_to_string(ESTADO_ESCALA_FILE).await {
        if let Ok(mut estado) = serde_json::from_str::<EstadoEscala>(&content) {
            if let Some(periodo_seguinte) = estado.periodo_seguinte.take() {
                estado.periodo_atual = periodo_seguinte;
                estado.status_trocas = "Fechado".to_string();
                println!("✅ Nova escala de {} a {} foi lançada com sucesso.", estado.periodo_atual.start_date, estado.periodo_atual.end_date);
            }
            
            if let Ok(json) = serde_json::to_string_pretty(&estado) {
                let _ = fs::write(ESTADO_ESCALA_FILE, json).await;
            }
        }
    }
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn adicionar_indisponibilidade_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<AdicionarIndisponibilidadeForm>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }
    
    let mut indisponibilidades: Vec<Indisponibilidade> = serde_json::from_str(&fs::read_to_string(INDISPONIBILIDADE_FILE).await.unwrap_or_else(|_| "[]".to_string())).unwrap_or_default();
    if !indisponibilidades.iter().any(|i| i.user_id == form.user_id && i.data == form.data) {
        indisponibilidades.push(Indisponibilidade {
            user_id: form.user_id,
            data: form.data,
            motivo: form.motivo,
        });
    }
    fs::write(INDISPONIBILIDADE_FILE, serde_json::to_string_pretty(&indisponibilidades).unwrap()).await.unwrap();
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn remover_indisponibilidade_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<RemoverIndisponibilidadeForm>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }
    
    let mut indisponibilidades: Vec<Indisponibilidade> = serde_json::from_str(&fs::read_to_string(INDISPONIBILIDADE_FILE).await.unwrap_or_else(|_| "[]".to_string())).unwrap_or_default();
    indisponibilidades.retain(|i| i.user_id != form.user_id || i.data != form.data);
    fs::write(INDISPONIBILIDADE_FILE, serde_json::to_string_pretty(&indisponibilidades).unwrap()).await.unwrap();
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn adicionar_punicao_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<AdicionarPunicaoForm>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }

    let mut punicoes: Vec<Punicao> = serde_json::from_str(&fs::read_to_string(PUNIDOS_FILE).await.unwrap_or_else(|_| "[]".to_string())).unwrap_or_default();

    if !punicoes.iter().any(|p| p.user_id == form.user_id) {
        punicoes.push(Punicao {
            user_id: form.user_id,
            total_a_cumprir: form.total_a_cumprir,
            ja_cumpridos: 0,
        });
    }

    fs::write(PUNIDOS_FILE, serde_json::to_string_pretty(&punicoes).unwrap()).await.unwrap();
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn remover_punicao_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<RemoverPunicaoForm>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }

    let mut punicoes: Vec<Punicao> = serde_json::from_str(&fs::read_to_string(PUNIDOS_FILE).await.unwrap_or_else(|_| "[]".to_string())).unwrap_or_default();
    punicoes.retain(|p| p.user_id != form.user_id);
    fs::write(PUNIDOS_FILE, serde_json::to_string_pretty(&punicoes).unwrap()).await.unwrap();
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn salvar_configuracao_punicao_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form_data): Form<Vec<(String, String)>>,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await { return Redirect::to("/"); }

    let postos_selecionados: Vec<String> = form_data
        .into_iter()
        .filter_map(|(key, value)| if key == "postos" { Some(value) } else { None })
        .collect();

    let config = ConfiguracaoEscala { postos_punicao: postos_selecionados };

    fs::write(CONFIGURACAO_FILE, serde_json::to_string_pretty(&config).unwrap()).await.unwrap();
    Redirect::to("/admin/escala")
}

#[debug_handler]
pub async fn gerar_pdf_escala_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    if !auth::has_role(&state, &cookies, "admin").await {
        return (StatusCode::FORBIDDEN, "Acesso negado.").into_response();
    }

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

    let user_logado = match users.get(&user_id) {
        Some(u) => u,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "Utilizador logado não encontrado.").into_response(),
    };
    let prioridade_cargos = ["Chefe de Dia", "Polícia", "Escalante", "Admin"];
    let mut cargo_dinamico = "Admin";

    for cargo in prioridade_cargos {
        if auth::has_role(&state, &cookies, cargo).await {
            cargo_dinamico = cargo;
            break;
        }
    }

    let pdf_data = escala_pdf::PdfData {
        periodo: periodo_ativo,
        escalas: &escalas_map,
        users: &users,
        info_assinatura_fixa: ("Nome Fixo", "Cargo Fixo"),
        info_assinatura_dinamica: (&user_logado.name, cargo_dinamico),
    };

    match escala_pdf::gerar_pdf_da_escala_ativa(pdf_data) {
        Ok(pdf_bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/pdf".parse().unwrap());
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"escala_{}_a_{}.pdf\"", periodo_ativo.start_date, periodo_ativo.end_date).parse().unwrap(),
            );
            (headers, pdf_bytes).into_response()
        }
        Err(e) => {
            eprintln!("Erro ao gerar PDF: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao gerar o PDF.").into_response()
        }
    }
}