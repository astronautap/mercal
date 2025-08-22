// src/cautela_handlers.rs

//! # Handlers para o Módulo de Cautela (Empréstimos)
//!
//! Este módulo contém os handlers HTTP para a interface do responsável,
//! utilizando um banco de dados SQLite e operando como uma Single Page Application (SPA).

use crate::auth::{AppState};
use crate::cautela::{self, Emprestimo, EventoEmprestimo, ItemCatalogo, Exemplar, StatusExemplar, TipoEvento, AtrasoInfo};
// ADICIONADO: Importar o novo módulo de views
use crate::views::cautela as view;
use axum::{
    debug_handler,
    extract::{Form, Query, State},
    response::{IntoResponse, Json, Redirect},
    http::StatusCode,
};
use chrono::{Local, NaiveDate, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio_rusqlite::Connection;
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;
use unidecode::unidecode;


// --- ESTRUTURAS PARA FORMULÁRIOS E QUERIES ---

#[derive(Debug, Deserialize)]
pub struct CautelaLoginForm { pub username: String, pub password: String }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AddItemForm { nome: String, setor: String, numero_identificacao: String }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AddExemplarForm { item_id: String, numero_identificacao: String }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeleteExemplarForm { item_id: String, numero_identificacao: String }

#[derive(Debug, Deserialize)]
pub struct SearchQuery { q: Option<String> }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmprestarForm {
    aluno_id: String,
    data_devolucao: NaiveDate,
    item_id: String,
    exemplar_id: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DevolverForm { emprestimo_id: String }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RenovarForm {
    emprestimo_id: String,
    nova_data_devolucao: NaiveDate,
}

#[derive(Debug, Deserialize)]
pub struct CatalogoQuery {
    setor: Option<String>,
}


// --- FUNÇÕES AUXILIARES ---

/// Normaliza um texto para busca (minúsculas, sem acentos/cedilha).
fn normalize_for_search(text: &str) -> String {
    unidecode(text).to_lowercase()
}

/// Verifica a autenticação do responsável através dos cookies da sessão.
async fn check_cautela_auth(state: &AppState, cookies: &Cookies) -> Result<String, impl IntoResponse> {
    if let Some(session_id) = cookies.get("cautela_session_id").map(|c| c.value().to_string()) {
        if state.sessions.lock().unwrap().contains(&session_id) {
            let username = cookies.get("cautela_user").map_or("".to_string(), |c| c.value().to_string());
            return Ok(username);
        }
    }
    Err(Redirect::to("/cautela"))
}


// --- O MÓDULO 'VIEW' FOI REMOVIDO DAQUI ---


// --- HANDLERS ---

#[debug_handler]
pub async fn cautela_login_page() -> impl IntoResponse { view::login_page(None) }

#[debug_handler]
pub async fn cautela_login_handler(State(state): State<AppState>, cookies: Cookies, Form(login): Form<CautelaLoginForm>) -> impl IntoResponse {
    let username_for_db = login.username.clone();
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let res: Result<Option<String>, _> = conn.call(move |conn| {
        let mut stmt = conn.prepare("SELECT password_hash FROM responsavel WHERE username = ?1")?;
        let mut rows = stmt.query_map([&username_for_db], |row| row.get(0))?;
        Ok(rows.next().transpose()?)
    }).await;
    if let Ok(Some(password_hash)) = res {
        if bcrypt::verify(&login.password, &password_hash).unwrap_or(false) {
            let session_id = Uuid::new_v4().to_string();
            state.sessions.lock().unwrap().insert(session_id.clone());
            cookies.add(Cookie::new("cautela_session_id", session_id));
            cookies.add(Cookie::new("cautela_user", login.username));
            return Redirect::to("/cautela/dashboard").into_response();
        }
    }
    view::login_page(Some("Utilizador ou palavra-passe incorretos.")).into_response()
}

#[debug_handler]
pub async fn cautela_logout_handler(State(state): State<AppState>, cookies: Cookies) -> impl IntoResponse {
    if let Some(cookie) = cookies.get("cautela_session_id") { state.sessions.lock().unwrap().remove(cookie.value()); }
    cookies.remove(Cookie::from("cautela_session_id"));
    cookies.remove(Cookie::from("cautela_user"));
    Redirect::to("/cautela").into_response()
}

#[debug_handler]
pub async fn cautela_dashboard_handler(
    State(state): State<AppState>, cookies: Cookies, Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let username = match check_cautela_auth(&state, &cookies).await { Ok(u) => u, Err(r) => return r.into_response() };
    let search_query = query.q.clone().unwrap_or_default();
    let users = state.users.lock().unwrap().clone();
    
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let (search_results, active_loans) = conn.call({
        let search_query = search_query.clone();
        let users = users.clone();
        move |conn| {
            let mut found_item_ids: HashSet<String> = HashSet::new();
            if !search_query.is_empty() {
                let normalized_query = normalize_for_search(&search_query);
                let mut stmt_items = conn.prepare("SELECT id FROM itens WHERE nome_normalizado LIKE ?1")?;
                for id_res in stmt_items.query_map([format!("%{normalized_query}%")], |row| row.get(0))? { found_item_ids.insert(id_res?); }
                let mut stmt_exemplar = conn.prepare("SELECT item_id FROM exemplares WHERE numero_identificacao = ?1")?;
                if let Ok(item_id) = stmt_exemplar.query_row([&search_query], |row| row.get(0)) { found_item_ids.insert(item_id); }
                let matching_users: Vec<String> = users.values().filter(|u| u.id == search_query || normalize_for_search(&u.name).contains(&normalized_query)).map(|u| u.id.clone()).collect();
                if !matching_users.is_empty() {
                    let params_sql = matching_users.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                    let sql = format!("SELECT ex.item_id FROM emprestimos e JOIN exemplares ex ON e.exemplar_id = ex.numero_identificacao WHERE e.aluno_id IN ({params_sql}) AND e.status = 'Emprestado'");
                    let mut stmt_alunos = conn.prepare(&sql)?;
                    for id_res in stmt_alunos.query_map(rusqlite::params_from_iter(matching_users), |row| row.get(0))? { found_item_ids.insert(id_res?); }
                }
            }
            let mut items_map = HashMap::new();
            if !found_item_ids.is_empty() {
                let item_ids: Vec<String> = found_item_ids.into_iter().collect();
                let params_sql = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let sql = format!("SELECT id, nome, setor FROM itens WHERE id IN ({params_sql})");
                let mut stmt = conn.prepare(&sql)?;
                for item_res in stmt.query_map(rusqlite::params_from_iter(item_ids.clone()), |row| Ok(ItemCatalogo { id: row.get(0)?, nome: row.get(1)?, setor: row.get(2)?, exemplares: vec![] }))? {
                    let item = item_res?;
                    items_map.insert(item.id.clone(), item);
                }
                let sql_ex = format!("SELECT item_id, numero_identificacao, status FROM exemplares WHERE item_id IN ({params_sql})");
                let mut stmt_ex = conn.prepare(&sql_ex)?;
                for ex_res in stmt_ex.query_map(rusqlite::params_from_iter(item_ids), |row| {
                    let status_str: String = row.get(2)?;
                    Ok((row.get::<_, String>(0)?, Exemplar { numero_identificacao: row.get(1)?, status: if status_str == "Disponivel" { StatusExemplar::Disponivel } else { StatusExemplar::Emprestado } }))
                })? {
                    let (item_id, exemplar) = ex_res?;
                    if let Some(item) = items_map.get_mut(&item_id) { item.exemplares.push(exemplar); }
                }
            }
            let mut stmt_loans = conn.prepare("SELECT e.id, ex.item_id, e.exemplar_id, e.aluno_id, h.data_devolucao_prevista FROM emprestimos e JOIN exemplares ex ON e.exemplar_id = ex.numero_identificacao JOIN historico_emprestimos h ON e.id = h.emprestimo_id WHERE e.status = 'Emprestado' AND h.id = (SELECT MAX(id) FROM historico_emprestimos WHERE emprestimo_id = e.id)")?;
            let mut loans = HashMap::new();
            for loan_res in stmt_loans.query_map([], |row| {
                let dev_date_str: String = row.get(4)?;
                let event = EventoEmprestimo { tipo: TipoEvento::Emprestimo, data_evento: Local::now(), data_devolucao_prevista: NaiveDate::parse_from_str(&dev_date_str, "%Y-%m-%d").unwrap(), responsavel_id: "".to_string() };
                Ok(Emprestimo { id: row.get(0)?, item_id: row.get(1)?, exemplar_id: row.get(2)?, aluno_id: row.get(3)?, status: StatusExemplar::Emprestado, historico: vec![event] })
            })? {
                let loan = loan_res?;
                loans.insert(loan.exemplar_id.clone(), loan);
            }
            Ok((items_map.into_values().collect::<Vec<_>>(), loans))
        }
    }).await.unwrap();
    view::dashboard_page(&username, &search_query, &search_results, &active_loans, &users).into_response()
}

#[debug_handler]
pub async fn cautela_catalogo_page(
    State(state): State<AppState>, cookies: Cookies, Query(query): Query<CatalogoQuery>,
) -> impl IntoResponse {
    if let Err(r) = check_cautela_auth(&state, &cookies).await { return r.into_response(); }
    let selected_setor = query.setor.clone();
    let users = state.users.lock().unwrap().clone();

    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let (items, setores, active_loans) = conn.call(move |conn| {
        let mut stmt_setores = conn.prepare("SELECT DISTINCT setor FROM itens ORDER BY setor")?;
        let setores_list: Vec<String> = stmt_setores.query_map([], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?;

        let items_res: Vec<ItemCatalogo> = {
            let mut sql = "SELECT id, nome, setor FROM itens".to_string();
            if let Some(ref setor) = selected_setor {
                sql.push_str(" WHERE setor = ?1 ORDER BY nome");
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map([setor], |row| Ok(ItemCatalogo { id: row.get(0)?, nome: row.get(1)?, setor: row.get(2)?, exemplares: vec![] }))?;
                rows.collect::<Result<_, _>>()?
            } else {
                sql.push_str(" ORDER BY nome");
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map([], |row| Ok(ItemCatalogo { id: row.get(0)?, nome: row.get(1)?, setor: row.get(2)?, exemplares: vec![] }))?;
                rows.collect::<Result<_, _>>()?
            }
        };
        let mut items = items_res;

        let mut stmt_ex = conn.prepare("SELECT item_id, numero_identificacao, status FROM exemplares")?;
        let mut exemplares_map: HashMap<String, Vec<Exemplar>> = HashMap::new();
        for res in stmt_ex.query_map([], |row| {
            let status_str: String = row.get(2)?;
            Ok((row.get::<_, String>(0)?, Exemplar { numero_identificacao: row.get(1)?, status: if status_str == "Disponivel" { StatusExemplar::Disponivel } else { StatusExemplar::Emprestado } }))
        })? {
            let (item_id, exemplar) = res?;
            exemplares_map.entry(item_id).or_default().push(exemplar);
        }
        for item in &mut items {
            if let Some(exemplares) = exemplares_map.remove(&item.id) { item.exemplares = exemplares; }
        }

        let mut stmt_loans = conn.prepare("SELECT e.id, ex.item_id, e.exemplar_id, e.aluno_id, h.data_devolucao_prevista FROM emprestimos e JOIN exemplares ex ON e.exemplar_id = ex.numero_identificacao JOIN historico_emprestimos h ON e.id = h.emprestimo_id WHERE e.status = 'Emprestado' AND h.id = (SELECT MAX(id) FROM historico_emprestimos WHERE emprestimo_id = e.id)")?;
        let mut loans = HashMap::new();
        for loan_res in stmt_loans.query_map([], |row| {
            let dev_date_str: String = row.get(4)?;
            let event = EventoEmprestimo { tipo: TipoEvento::Emprestimo, data_evento: Local::now(), data_devolucao_prevista: NaiveDate::parse_from_str(&dev_date_str, "%Y-%m-%d").unwrap(), responsavel_id: "".to_string() };
            Ok(Emprestimo { id: row.get(0)?, item_id: row.get(1)?, exemplar_id: row.get(2)?, aluno_id: row.get(3)?, status: StatusExemplar::Emprestado, historico: vec![event] })
        })? {
            let loan = loan_res?;
            loans.insert(loan.exemplar_id.clone(), loan);
        }
        
        Ok((items, setores_list, loans))
    }).await.unwrap();

    view::catalogo_page(&items, &setores, query.setor.as_deref(), &active_loans, &users).into_response()
}

#[debug_handler]
pub async fn cautela_add_item_handler(State(state): State<AppState>, cookies: Cookies, Form(form): Form<AddItemForm>) -> impl IntoResponse {
    if let Err(r) = check_cautela_auth(&state, &cookies).await { return r.into_response(); }

    let conn = match Connection::open(cautela::DB_FILE).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("Falha ao conectar à base de dados: {}", e)).into_response(),
    };

    // CORREÇÃO: Substituir .unwrap() por um tratamento de erro robusto
    let res = conn.call(move |conn| {
        let item_id = Uuid::new_v4().to_string();
        let nome_normalizado = normalize_for_search(&form.nome);

        // Iniciar uma transação para garantir que ambas as inserções funcionem ou nenhuma funcione.
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO itens (id, nome, setor, nome_normalizado) VALUES (?1, ?2, ?3, ?4)", params![&item_id, &form.nome, &form.setor, &nome_normalizado])?;
        tx.execute("INSERT INTO exemplares (numero_identificacao, item_id, status) VALUES (?1, ?2, 'Disponivel')", params![&form.numero_identificacao, &item_id])?;
        
        // tx.commit() retorna um Result que precisa ser tratado.
        Ok(tx.commit())
    }).await;

    match res {
        // Sucesso tanto da conexão quanto da transação
        Ok(Ok(_)) => Redirect::to("/cautela/catalogo").into_response(),
        // Erro na transação (ex: ID de exemplar duplicado)
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, format!("Erro ao executar transação: {}", e)).into_response(),
        // Erro na comunicação com a thread da base de dados
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Erro de comunicação com a base de dados: {}", e)).into_response(),
    }
}

#[debug_handler]
pub async fn cautela_add_exemplar_handler(
    State(state): State<AppState>, cookies: Cookies, Json(form): Json<AddExemplarForm>
) -> impl IntoResponse {
    if let Err(r) = check_cautela_auth(&state, &cookies).await { return r.into_response(); }
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let res = conn.call(move |conn| {
        conn.execute("INSERT OR IGNORE INTO exemplares (numero_identificacao, item_id, status) VALUES (?1, ?2, 'Disponivel')", params![form.numero_identificacao, form.item_id])?;
        Ok(form)
    }).await;
    match res {
        Ok(form) => (StatusCode::CREATED, Json(form)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[debug_handler]
pub async fn cautela_delete_exemplar_handler(
    State(state): State<AppState>, cookies: Cookies, Json(form): Json<DeleteExemplarForm>
) -> impl IntoResponse {
    if let Err(r) = check_cautela_auth(&state, &cookies).await { return r.into_response(); }
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let res = conn.call(move |conn| {
        let changed = conn.execute("DELETE FROM exemplares WHERE item_id = ?1 AND numero_identificacao = ?2 AND status = 'Disponivel'", params![form.item_id, form.numero_identificacao])?;
        if changed == 0 { 
            Err(tokio_rusqlite::Error::from(rusqlite::Error::QueryReturnedNoRows))
        } else { 
            Ok(()) 
        }
    }).await;
    match res {
        Ok(_) => (StatusCode::OK, "Removido").into_response(),
        Err(_) => (StatusCode::BAD_REQUEST, "Exemplar não encontrado ou não está disponível.").into_response(),
    }
}

pub async fn cautela_emprestar_handler(
    State(state): State<AppState>, cookies: Cookies, Json(form): Json<EmprestarForm>
) -> impl IntoResponse {
    let responsavel_id = match check_cautela_auth(&state, &cookies).await { Ok(u) => u, Err(r) => return r.into_response() };
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let emprestimo_id = Uuid::new_v4().to_string();
    let form_data = form.clone();
    let emprestimo_id_clone = emprestimo_id.clone();
    let res: Result<(), tokio_rusqlite::Error> = conn.call(move |conn| {
        let tx = conn.transaction()?;
        let updated_rows = tx.execute("UPDATE exemplares SET status = 'Emprestado' WHERE numero_identificacao = ?1 AND status = 'Disponivel'", [&form_data.exemplar_id])?;
        
        if updated_rows == 0 {
            return Err(tokio_rusqlite::Error::from(rusqlite::Error::QueryReturnedNoRows));
        }

        tx.execute("INSERT INTO emprestimos (id, exemplar_id, aluno_id, status) VALUES (?1, ?2, ?3, 'Emprestado')", params![&emprestimo_id_clone, &form_data.exemplar_id, &form_data.aluno_id])?;
        tx.execute("INSERT INTO historico_emprestimos (emprestimo_id, tipo_evento, data_evento, data_devolucao_prevista, responsavel_id) VALUES (?1, 'Emprestimo', ?2, ?3, ?4)", params![&emprestimo_id_clone, Utc::now().to_rfc3339(), form_data.data_devolucao.to_string(), responsavel_id])?;
        tx.commit()?;
        Ok(())
    }).await;

    if res.is_ok() {
        let evento = EventoEmprestimo { tipo: TipoEvento::Emprestimo, data_evento: Local::now(), data_devolucao_prevista: form.data_devolucao, responsavel_id: "".into() };
        let emprestimo_res = Emprestimo { id: emprestimo_id, item_id: form.item_id, exemplar_id: form.exemplar_id, aluno_id: form.aluno_id, status: StatusExemplar::Emprestado, historico: vec![evento] };
        
        (StatusCode::CREATED, Json(emprestimo_res)).into_response()
    } else {
        (StatusCode::BAD_REQUEST, "Erro ao registrar empréstimo. O item pode já não estar disponível.").into_response()
    }
}


#[debug_handler]
pub async fn cautela_devolver_handler(
    State(state): State<AppState>, cookies: Cookies, Json(form): Json<DevolverForm>
) -> impl IntoResponse {
    let responsavel_id = match check_cautela_auth(&state, &cookies).await { Ok(u) => u, Err(r) => return r.into_response() };
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let res: Result<(String, String), _> = conn.call(move |conn| {
        let tx = conn.transaction()?;
        let (exemplar_id, item_id): (String, String) = tx.query_row("SELECT e.exemplar_id, ex.item_id FROM emprestimos e JOIN exemplares ex ON e.exemplar_id = ex.numero_identificacao WHERE e.id = ?1", [&form.emprestimo_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let last_dev_date: String = tx.query_row("SELECT data_devolucao_prevista FROM historico_emprestimos WHERE emprestimo_id = ?1 ORDER BY id DESC LIMIT 1", [&form.emprestimo_id], |row| row.get(0))?;
        tx.execute("UPDATE exemplares SET status = 'Disponivel' WHERE numero_identificacao = ?1", [&exemplar_id])?;
        tx.execute("UPDATE emprestimos SET status = 'Devolvido' WHERE id = ?1", [&form.emprestimo_id])?;
        tx.execute("INSERT INTO historico_emprestimos (emprestimo_id, tipo_evento, data_evento, data_devolucao_prevista, responsavel_id) VALUES (?1, 'Devolucao', ?2, ?3, ?4)", params![&form.emprestimo_id, Utc::now().to_rfc3339(), last_dev_date, responsavel_id])?;
        tx.commit()?;
        Ok((exemplar_id, item_id))
    }).await;
    match res {
        Ok((exemplar_id, item_id)) => {
            #[derive(Serialize)] struct DevolucaoResponse { exemplar_id: String, item_id: String }
            (StatusCode::OK, Json(DevolucaoResponse { exemplar_id, item_id })).into_response()
        },
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao registrar devolução").into_response()
    }
}

#[debug_handler]
pub async fn cautela_renovar_handler(
    State(state): State<AppState>, cookies: Cookies, Json(form): Json<RenovarForm>
) -> impl IntoResponse {
    let responsavel_id = match check_cautela_auth(&state, &cookies).await { Ok(u) => u, Err(r) => return r.into_response() };
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let res: Result<Emprestimo, _> = conn.call(move |conn| {
        conn.execute("INSERT INTO historico_emprestimos (emprestimo_id, tipo_evento, data_evento, data_devolucao_prevista, responsavel_id) VALUES (?1, 'Renovacao', ?2, ?3, ?4)", params![&form.emprestimo_id, Utc::now().to_rfc3339(), form.nova_data_devolucao.to_string(), responsavel_id])?;
        let (item_id, exemplar_id, aluno_id): (String, String, String) = conn.query_row("SELECT ex.item_id, e.exemplar_id, e.aluno_id FROM emprestimos e JOIN exemplares ex ON e.exemplar_id = ex.numero_identificacao WHERE e.id = ?1", [&form.emprestimo_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let evento = EventoEmprestimo { tipo: TipoEvento::Renovacao, data_evento: Local::now(), data_devolucao_prevista: form.nova_data_devolucao, responsavel_id: "".into() };
        Ok(Emprestimo { id: form.emprestimo_id, item_id, exemplar_id, aluno_id, status: StatusExemplar::Emprestado, historico: vec![evento] })
    }).await;
    match res {
        Ok(emprestimo) => (StatusCode::OK, Json(emprestimo)).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao renovar empréstimo").into_response(),
    }
}

#[debug_handler]
pub async fn cautela_atrasos_page(State(state): State<AppState>, cookies: Cookies) -> impl IntoResponse {
    if let Err(r) = check_cautela_auth(&state, &cookies).await { return r.into_response(); }
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let mut atrasos = conn.call(move |conn| {
        let mut stmt = conn.prepare("SELECT e.aluno_id, i.nome, e.exemplar_id, h.data_devolucao_prevista FROM emprestimos e JOIN exemplares ex ON e.exemplar_id = ex.numero_identificacao JOIN itens i ON ex.item_id = i.id JOIN historico_emprestimos h ON e.id = h.emprestimo_id WHERE e.status = 'Emprestado' AND h.id = (SELECT MAX(id) FROM historico_emprestimos WHERE emprestimo_id = e.id) AND h.data_devolucao_prevista < date('now')")?;
        let today = Local::now().date_naive();
        let mut atrasos_info = Vec::new();
        for row in stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)))? {
            let (aluno_id, item_nome, exemplar_id, dev_date_str) = row?;
            let dev_date = NaiveDate::parse_from_str(&dev_date_str, "%Y-%m-%d").unwrap();
            atrasos_info.push(AtrasoInfo { nome_aluno: "".to_string(), id_aluno: aluno_id, nome_item: item_nome, exemplar_id, data_devolucao: dev_date, dias_atrasado: (today - dev_date).num_days() });
        }
        Ok(atrasos_info)
    }).await.unwrap();
    let users = state.users.lock().unwrap().clone();
    for a in &mut atrasos {
        a.nome_aluno = users.get(&a.id_aluno).map_or("N/A".to_string(), |u| u.name.clone());
    }
    atrasos.sort_by(|a, b| b.dias_atrasado.cmp(&a.dias_atrasado));
    view::atrasos_page(&atrasos).into_response()
}

#[debug_handler]
pub async fn teste_json_handler() -> impl IntoResponse {
    #[derive(serde::Serialize)]
    struct MensagemTeste {
        status: String,
        mensagem: String,
    }

    let payload = MensagemTeste {
        status: "OK".to_string(),
        mensagem: "Se vir este JSON, a configuração base está correta.".to_string(),
    };

    (StatusCode::OK, Json(payload)).into_response()
}