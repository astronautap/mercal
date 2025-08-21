// src/cautela_handlers.rs

//! # Handlers para o Módulo de Cautela (Empréstimos)
//!
//! Este módulo contém os handlers HTTP para a interface do responsável,
//! utilizando um banco de dados SQLite e operando como uma Single Page Application (SPA).

use crate::auth::{AppState, User};
use crate::cautela::{self, Emprestimo, EventoEmprestimo, ItemCatalogo, Exemplar, StatusExemplar, TipoEvento};
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
// Adicionado `Serialize` e `Clone` onde necessário para a comunicação via API JSON.

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

/// Estrutura para informações de itens em atraso.
pub struct AtrasoInfo {
    pub nome_aluno: String,
    pub id_aluno: String,
    pub nome_item: String,
    pub exemplar_id: String,
    pub data_devolucao: NaiveDate,
    pub dias_atrasado: i64,
}

// --- MÓDULO DE VISUALIZAÇÃO (HTML + JAVASCRIPT) ---
mod view {
    use super::{AtrasoInfo, Emprestimo, ItemCatalogo, StatusExemplar, User};
    use axum::response::Html;
    use chrono::Local;
    use std::collections::HashMap;

    
    const CSS: &str = r#"
        :root { 
            --primary-color: #00695c; --primary-dark: #004d40; --background-color: #e0f2f1; 
            --card-background: #ffffff; --text-color: #333; --text-light: #757575;
            --border-color: #b2dfdb; --danger-color: #c62828; --success-color: #2e7d32;
        }
        body { font-family: 'Roboto', sans-serif; background-color: var(--background-color); margin: 0; }
        .container { max-width: 1000px; margin: 5vh auto; padding: 20px; }
        a { color: var(--primary-color); text-decoration: none; font-weight: bold; }
        .login-card { background: var(--card-background); max-width: 400px; margin: 15vh auto; border-radius: 12px; box-shadow: 0 10px 25px rgba(0,0,0,0.1); padding: 40px; text-align: center; border-top: 5px solid var(--primary-color); }
        .login-header h1 { margin: 0; font-size: 1.8em; color: var(--text-color); }
        .login-header p { color: var(--text-light); }
        input[type="text"], input[type="password"], input[type="date"] { width: 100%; padding: 14px; margin: 10px 0; border: 1px solid #e0e0e0; border-radius: 4px; box-sizing: border-box; font-size: 16px; }
        button { padding: 14px; color: white; border: none; border-radius: 5px; cursor: pointer; font-size: 16px; background-color: var(--primary-color); font-weight: bold; transition: background-color 0.2s; }
        button:hover { background-color: var(--primary-dark); }
        .btn-danger { background-color: var(--danger-color); }
        .btn-danger:hover { background-color: #b71c1c; }
        .header { display: flex; justify-content: space-between; align-items: center; background: var(--card-background); padding: 15px 30px; border-radius: 8px; box-shadow: 0 4px 15px rgba(0,0,0,0.08); margin-bottom: 30px; }
        .header h1 { margin: 0; color: var(--primary-color); font-size: 1.5em; }
        .header .nav a { margin-left: 20px; }
        .card { background: var(--card-background); border-radius: 8px; box-shadow: 0 4px 15px rgba(0,0,0,0.08); padding: 25px; margin-bottom: 25px; }
        .card h2 { margin-top: 0; padding-bottom: 10px; border-bottom: 2px solid var(--border-color); color: var(--primary-dark); }
        table { width: 100%; border-collapse: collapse; margin-top: 15px; }
        th, td { border: 1px solid var(--border-color); padding: 10px; text-align: left; vertical-align: middle; }
        th { background-color: #e0f2f1; }
        .form-inline { display: flex; gap: 10px; align-items: center; padding-bottom: 10px; }
        .status-Disponivel { color: var(--success-color); font-weight: bold; }
        .status-Emprestado { color: var(--text-light); }
        .overdue { color: var(--danger-color); font-weight: bold; }
        
        /* [NOVO] Estilos para o dropdown do catálogo */
        details.item-details > summary {
            font-size: 1.25em;
            font-weight: 500;
            color: var(--primary-dark);
            padding: 15px 0;
            cursor: pointer;
            list-style: none; /* Remove a seta padrão em alguns navegadores */
            outline: none;
        }
        details.item-details > summary::-webkit-details-marker { display: none; } /* Remove a seta no Chrome/Safari */
        details.item-details > summary::before {
            content: '▸';
            margin-right: 10px;
            display: inline-block;
            transition: transform 0.2s;
        }
        details[open].item-details > summary::before {
            transform: rotate(90deg);
        }
        .item-content {
            padding-left: 25px; /* Indenta o conteúdo do dropdown */
            border-left: 3px solid var(--border-color);
            padding-top: 10px;
        }
    "#;

    fn render_page(title: &str, content: String) -> Html<String> {
        Html(format!(r#"<!DOCTYPE html><html lang="pt-BR"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>{title}</title><link href="https://fonts.googleapis.com/css2?family=Roboto:wght@400;500;700&display=swap" rel="stylesheet"><style>{CSS}</style></head><body><div class="container">{content}</div></body></html>"#))
    }

    pub fn login_page(error: Option<&str>) -> Html<String> {
        let error_html = error.map(|e| format!("<p style='color: #d32f2f;'>{}</p>", e)).unwrap_or_default();
        let content = format!(r#"<div class="login-card"><div class="login-header"><h1>Acesso Restrito</h1><p>Paiol de Livros / Cautela</p></div><form method="POST" action="/cautela/login"><input type="text" name="username" placeholder="Utilizador" required /><input type="password" name="password" placeholder="Palavra-passe" required />{error_html}<button type="submit" style="width: 100%;">Entrar</button></form></div>"#);
        render_page("Login - Cautela", content)
    }
    
    // Função catalogo_page atualizada
    pub fn catalogo_page(
        catalogo: &[ItemCatalogo], 
        setores: &[String], 
        selected_setor: Option<&str>,
        active_loans: &HashMap<String, Emprestimo>, // Novo parâmetro
        users: &HashMap<String, User>,             // Novo parâmetro
    ) -> Html<String> {
        let mut items_html = String::new();
        for item in catalogo {
            let exemplares_html = item.exemplares.iter().map(|ex| {
                let delete_button = if ex.status == StatusExemplar::Disponivel {
                    format!(r#"
                        <form class="form-delete-exemplar" onsubmit="return confirm('Tem a certeza que deseja remover este exemplar?');">
                            <input type="hidden" name="item_id" value="{item_id}">
                            <input type="hidden" name="numero_identificacao" value="{ex_id}">
                            <button type="submit" class="btn-danger" style="padding: 5px 10px; font-size: 12px;">X</button>
                        </form>"#, item_id = item.id, ex_id = ex.numero_identificacao)
                } else {
                    "".to_string()
                };

                // [LÓGICA ATUALIZADA] Verifica o status e busca informações do empréstimo
                let (status_class, status_html) = match ex.status {
                    StatusExemplar::Disponivel => ("status-Disponivel", "Disponível".to_string()),
                    StatusExemplar::Emprestado => {
                        let info = active_loans.get(&ex.numero_identificacao)
                            .map(|e| {
                                let aluno = users.get(&e.aluno_id).map_or("?", |u| u.name.as_str());
                                let dev_date = e.historico.last().unwrap().data_devolucao_prevista;
                                let overdue_class = if dev_date < Local::now().date_naive() { "overdue" } else { "" };
                                format!("Emprestado para: <strong>{}</strong> (Dev. <span class='{}'>{}</span>)", 
                                    aluno, overdue_class, dev_date.format("%d/%m/%Y"))
                            })
                            .unwrap_or_else(|| "Informação indisponível".to_string());
                        ("status-Emprestado", info)
                    }
                };
                
                format!(r#"
                    <tr data-exemplar-id="{ex_id}">
                        <td>{ex_id}</td>
                        <td class="{status_class}">{status_html}</td>
                        <td>{delete_button}</td>
                    </tr>"#, 
                    ex_id = ex.numero_identificacao
                )
            }).collect::<String>();
            
            // [ESTRUTURA ATUALIZADA] Usando <details> e <summary> para o dropdown
            items_html.push_str(&format!(r#"
                <div class="card" style="padding: 0 25px;">
                    <details class="item-details">
                        <summary>
                            {nome} <span style="font-weight: normal; color: var(--text-light); font-size: 0.8em;">({setor})</span>
                        </summary>
                        <div class="item-content">
                            <table>
                                <thead><tr><th>Nº do Exemplar</th><th>Status</th><th>Ação</th></tr></thead>
                                <tbody id="exemplares-tbody-{id}">{exemplares_html}</tbody>
                            </table>
                            <hr style="border: 1px solid #e0f2f1; margin: 20px 0;">
                            <form class="form-add-exemplar form-inline" data-item-id="{id}">
                                <input type="text" name="numero_identificacao" placeholder="Novo Nº de exemplar" required style="flex-grow:1; margin:0;">
                                <button type="submit" style="white-space: nowrap; margin:0;">Adicionar Exemplar</button>
                            </form>
                        </div>
                    </details>
                </div>"#, 
                nome = item.nome, setor = item.setor, id = item.id
            ));
        }

        let mut options_html = "<option value=''>-- Todos os Setores --</option>".to_string();
        for setor in setores {
            let selected_attr = if Some(setor.as_str()) == selected_setor { " selected" } else { "" };
            options_html.push_str(&format!(r#"<option value="{setor}"{selected_attr}>{setor}</option>"#));
        }
        let filter_html = format!(r#"
            <div class="card">
                <h2>Filtrar Catálogo por Setor</h2>
                <form method="GET" action="/cautela/catalogo" class="form-inline">
                    <select name="setor" onchange="this.form.submit()" style="flex-grow: 1; padding: 14px; font-size: 16px;">{options_html}</select>
                </form>
            </div>
        "#);

        let script_js = r#"
            <script>
                document.addEventListener('submit', async function(event) {
                    const form = event.target;

                    // --- Adicionar Exemplar ---
                    if (form.classList.contains('form-add-exemplar')) {
                        event.preventDefault();
                        const itemId = form.dataset.itemId;
                        const input = form.querySelector('input[name="numero_identificacao"]');
                        const numeroIdentificacao = input.value;
                        if (!numeroIdentificacao) return;

                        const response = await fetch('/cautela/catalogo/add-exemplar', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ item_id: itemId, numero_identificacao: numeroIdentificacao })
                        });

                        if (response.ok) {
                            const novoExemplar = await response.json();
                            const tbody = document.getElementById(`exemplares-tbody-${itemId}`);
                            const newRow = tbody.insertRow();
                            newRow.dataset.exemplarId = novoExemplar.numero_identificacao;
                            newRow.innerHTML = `
                                <td>${novoExemplar.numero_identificacao}</td>
                                <td class="status-Disponivel">Disponível</td>
                                <td>
                                    <form class="form-delete-exemplar" onsubmit="return confirm('Tem a certeza?');">
                                        <input type="hidden" name="item_id" value="${itemId}">
                                        <input type="hidden" name="numero_identificacao" value="${novoExemplar.numero_identificacao}">
                                        <button type="submit" class="btn-danger" style="padding: 5px 10px; font-size: 12px;">X</button>
                                    </form>
                                </td>`;
                            input.value = '';
                        } else { alert('Erro ao adicionar exemplar.'); }
                    }

                    // --- Deletar Exemplar ---
                    if (form.classList.contains('form-delete-exemplar')) {
                        event.preventDefault();
                        const itemId = form.querySelector('input[name="item_id"]').value;
                        const numeroIdentificacao = form.querySelector('input[name="numero_identificacao"]').value;
                        
                        const response = await fetch('/cautela/catalogo/delete-exemplar', {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ item_id: itemId, numero_identificacao: numeroIdentificacao })
                        });

                        if (response.ok) {
                            const row = form.closest('tr');
                            row.remove();
                        } else { alert('Erro ao remover exemplar. Verifique se não está emprestado.'); }
                    }
                });
            </script>
        "#;

        let content = format!(r#"
            <div class="header"><h1>Gestão de Catálogo</h1><div class="nav"><a href="/cautela/dashboard">Voltar ao Painel</a><a href="/cautela/logout">Sair</a></div></div>
            <div class="card"><h2>Adicionar Novo Item ao Catálogo</h2><form method="POST" action="/cautela/catalogo/add-item"><div style="display: flex; gap: 20px; align-items: center;"><input type="text" name="nome" placeholder="Nome do Item (ex: Abacate Voador)" required style="flex-grow: 2; margin: 0;"><input type="text" name="setor" placeholder="Setor (ex: Biblioteca de Exatas)" required style="flex-grow: 1; margin: 0;"><input type="text" name="numero_identificacao" placeholder="Nº do 1º Exemplar" required style="flex-grow: 1; margin: 0;"><button type="submit" style="margin: 0;">Adicionar Item</button></div></form></div>
            {filter_html}
            {items_html}
            {script_js}
        "#);
        render_page("Gestão de Catálogo", content)
    }

    pub fn dashboard_page(
        username: &str, search_query: &str, search_results: &[ItemCatalogo],
        active_loans: &HashMap<String, Emprestimo>, users: &HashMap<String, User>,
    ) -> Html<String> {
        let results_html = if search_results.is_empty() {
            if search_query.is_empty() { "<p>Comece por procurar um item, exemplar ou aluno acima.</p>".to_string() }
            else { format!("<p>Nenhum resultado encontrado para '<strong>{search_query}</strong>'.</p>") }
        } else {
            search_results.iter().map(|item| {
                let exemplares_html = item.exemplares.iter().map(|ex| {
                    let (status_class, status_text, actions_html) = match ex.status {
                        StatusExemplar::Disponivel => ("Disponivel", "Disponível".to_string(), format!(r#"
                            <form class="form-emprestar">
                                <input type="hidden" name="item_id" value="{item_id}">
                                <input type="hidden" name="exemplar_id" value="{ex_id}">
                                <input type="text" name="aluno_id" placeholder="ID Aluno" required style="margin:0; flex-grow:1;">
                                <input type="date" name="data_devolucao" required style="margin:0;">
                                <button type="submit" style="padding: 10px 15px; margin:0;">Emprestar</button>
                            </form>"#, item_id = item.id, ex_id = ex.numero_identificacao)),
                        StatusExemplar::Emprestado => {
                            let emprestimo = active_loans.get(&ex.numero_identificacao);
                            let info = emprestimo.map(|e| {
                                let aluno = users.get(&e.aluno_id).map_or("?", |u| &u.name);
                                let dev_date = e.historico.last().unwrap().data_devolucao_prevista;
                                let overdue_class = if dev_date < Local::now().date_naive() { "overdue" } else { "" };
                                format!("Para: {aluno} (Dev. <span class='{overdue_class}'>{}</span>)", dev_date.format("%d/%m/%Y"))
                            }).unwrap_or_else(|| "Informação indisponível".to_string());
                            let actions = emprestimo.map(|e| {
                                format!(r#"
                                    <div style="display: flex; gap: 5px; align-items: center;">
                                        <form class="form-devolver" style="margin:0;"><input type="hidden" name="emprestimo_id" value="{id}"><button type="submit" class="btn-danger" style="padding: 5px 10px;">Devolver</button></form>
                                        <form class="form-renovar" style="margin:0;"><input type="hidden" name="emprestimo_id" value="{id}"><input type="date" name="nova_data_devolucao" required style="padding: 5px; font-size: 12px;"><button type="submit" style="padding: 5px 10px;">Renovar</button></form>
                                    </div>"#, id = e.id)
                            }).unwrap_or_default();
                            ("Emprestado", info, actions)
                        }
                    };
                    format!(r#"
                        <tr data-exemplar-id="{ex_id}">
                            <td>{ex_id}</td><td class="status-cell status-{status_class}">{status_text}</td><td class="actions-cell">{actions_html}</td>
                        </tr>"#, ex_id = ex.numero_identificacao)
                }).collect::<String>();
                format!("<div class='card'><h2>{}</h2><table><thead><tr><th>Nº Exemplar</th><th>Status</th><th>Ações</th></tr></thead><tbody>{}</tbody></table></div>", item.nome, exemplares_html)
            }).collect()
        };

        let users_json = serde_json::to_string(users).unwrap_or_else(|_| "{}".to_string());

        let script_js = format!(r#"
            <script>
                const USERS = {users_json};

                document.getElementById('search-results-container').addEventListener('submit', async function(event) {{
                    const form = event.target;
                    event.preventDefault(); // Previne o recarregamento para todas as ações
                    
                    if (form.classList.contains('form-emprestar')) {{
                        const data = {{
                            item_id: form.querySelector('[name=item_id]').value,
                            exemplar_id: form.querySelector('[name=exemplar_id]').value,
                            aluno_id: form.querySelector('[name=aluno_id]').value,
                            data_devolucao: form.querySelector('[name=data_devolucao]').value
                        }};
                        const response = await fetch('/cautela/emprestar', {{ method: 'POST', headers: {{ 'Content-Type': 'application/json' }}, body: JSON.stringify(data) }});
                        if (response.ok) {{ updateExemplarRow(data.exemplar_id, await response.json()); }} 
                        else {{ alert('Erro ao emprestar item.'); }}
                    }}

                    if (form.classList.contains('form-devolver')) {{
                        const data = {{ emprestimo_id: form.querySelector('[name=emprestimo_id]').value }};
                        const response = await fetch('/cautela/devolver', {{ method: 'POST', headers: {{ 'Content-Type': 'application/json' }}, body: JSON.stringify(data) }});
                        if (response.ok) {{ const {{ exemplar_id, item_id }} = await response.json(); updateExemplarRow(exemplar_id, null, item_id); }} 
                        else {{ alert('Erro ao devolver item.'); }}
                    }}

                    if (form.classList.contains('form-renovar')) {{
                        const data = {{ emprestimo_id: form.querySelector('[name=emprestimo_id]').value, nova_data_devolucao: form.querySelector('[name=nova_data_devolucao]').value }};
                        const response = await fetch('/cautela/renovar', {{ method: 'POST', headers: {{ 'Content-Type': 'application/json' }}, body: JSON.stringify(data) }});
                        if (response.ok) {{ 
                            const emprestimo = await response.json();
                            updateExemplarRow(emprestimo.exemplar_id, emprestimo);
                        }} else {{ alert('Erro ao renovar empréstimo.'); }}
                    }}
                }});

                function updateExemplarRow(exemplarId, emprestimo, itemIdForDevolucao) {{
                    const row = document.querySelector(`tr[data-exemplar-id="${{exemplarId}}"]`);
                    if (!row) return;
                    const statusCell = row.querySelector('.status-cell');
                    const actionsCell = row.querySelector('.actions-cell');

                    if (emprestimo) {{ // Emprestado ou Renovado
                        const aluno = USERS[emprestimo.aluno_id] ? USERS[emprestimo.aluno_id].name : '?';
                        const devDate = new Date(emprestimo.historico[emprestimo.historico.length - 1].data_devolucao_prevista + 'T00:00:00');
                        const devDateFormatted = devDate.toLocaleDateString('pt-BR');
                        const isOverdue = new Date(devDate.toDateString()) < new Date(new Date().toDateString());
                        statusCell.className = 'status-cell status-Emprestado';
                        statusCell.innerHTML = `Para: ${{aluno}} (Dev. <span class="${{isOverdue ? 'overdue' : ''}}">${{devDateFormatted}}</span>)`;
                        actionsCell.innerHTML = `
                            <div style="display: flex; gap: 5px; align-items: center;">
                                <form class="form-devolver" style="margin:0;"><input type="hidden" name="emprestimo_id" value="${{emprestimo.id}}"><button type="submit" class="btn-danger" style="padding: 5px 10px;">Devolver</button></form>
                                <form class="form-renovar" style="margin:0;"><input type="hidden" name="emprestimo_id" value="${{emprestimo.id}}"><input type="date" name="nova_data_devolucao" required style="padding: 5px; font-size: 12px;"><button type="submit" style="padding: 5px 10px;">Renovar</button></form>
                            </div>`;
                    }} else {{ // Devolvido
                        statusCell.className = 'status-cell status-Disponivel';
                        statusCell.textContent = 'Disponível';
                        actionsCell.innerHTML = `
                            <form class="form-emprestar">
                                <input type="hidden" name="item_id" value="${{itemIdForDevolucao}}">
                                <input type="hidden" name="exemplar_id" value="${{exemplarId}}">
                                <input type="text" name="aluno_id" placeholder="ID Aluno" required style="margin:0; flex-grow:1;">
                                <input type="date" name="data_devolucao" required style="margin:0;">
                                <button type="submit" style="padding: 10px 15px; margin:0;">Emprestar</button>
                            </form>`;
                    }}
                }}
            </script>
        "#);

        let content = format!(r#"
            <div class="header"><h1>Painel da Cautela</h1><span>Bem-vindo, <strong>{username}</strong>!</span><div class="nav"><a href="/cautela/catalogo">Catálogo</a><a href="/cautela/atrasos">Atrasos</a><a href="/cautela/logout">Sair</a></div></div>
            <div class="card"><h2>Ações de Empréstimo</h2><form method="GET" action="/cautela/dashboard" class="form-inline"><input type="text" name="q" placeholder="Procurar por item, ID de exemplar ou aluno..." value="{search_query}" style="flex-grow: 1;"><button type="submit">Procurar</button></form></div>
            <div id="search-results-container">{results_html}</div>
            {script_js}
        "#);
        render_page("Dashboard - Cautela", content)
    }

    pub fn atrasos_page(atrasos: &[AtrasoInfo]) -> Html<String> {
        let table_rows = if atrasos.is_empty() {
            "<tr><td colspan='4' style='text-align: center;'>Não há nenhum item em atraso.</td></tr>".to_string()
        } else {
            atrasos.iter().map(|a| {
                format!(r#"<tr><td>{} ({})</td><td>{} ({})</td><td>{}</td><td class="overdue">{} dias</td></tr>"#,
                    a.nome_aluno, a.id_aluno, a.nome_item, a.exemplar_id, a.data_devolucao.format("%d/%m/%Y"), a.dias_atrasado)
            }).collect::<String>()
        };
        let content = format!(r#"<div class="header"><h1>Relatório de Atrasos</h1><div class="nav"><a href="/cautela/dashboard">Voltar ao Painel</a><a href="/cautela/logout">Sair</a></div></div><div class="card"><h2>Itens com Devolução Pendente</h2><table><thead><tr><th>Aluno</th><th>Item</th><th>Data de Devolução</th><th>Dias Atrasado</th></tr></thead><tbody>{table_rows}</tbody></table></div>"#);
        render_page("Relatório de Atrasos", content)
    }
}

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
    let users = state.users.lock().unwrap().clone(); // Pegamos os usuários aqui

    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    let (items, setores, active_loans) = conn.call(move |conn| {
        // Busca de setores (inalterado)
        let mut stmt_setores = conn.prepare("SELECT DISTINCT setor FROM itens ORDER BY setor")?;
        let setores_list: Vec<String> = stmt_setores.query_map([], |row| row.get(0))?.collect::<Result<Vec<_>, _>>()?;

        // Busca de itens do catálogo (inalterado)
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

        // Busca de exemplares (inalterado)
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

        // [NOVO] Busca por empréstimos ativos, similar ao dashboard
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

    // Passamos os novos dados (active_loans e users) para a função de renderização da view
    view::catalogo_page(&items, &setores, query.setor.as_deref(), &active_loans, &users).into_response()
}

#[debug_handler]
pub async fn cautela_add_item_handler(State(state): State<AppState>, cookies: Cookies, Form(form): Form<AddItemForm>) -> impl IntoResponse {
    if let Err(r) = check_cautela_auth(&state, &cookies).await { return r.into_response(); }
    let conn = Connection::open(cautela::DB_FILE).await.unwrap();
    conn.call(move |conn| {
        let item_id = Uuid::new_v4().to_string();
        let nome_normalizado = normalize_for_search(&form.nome);
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO itens (id, nome, setor, nome_normalizado) VALUES (?1, ?2, ?3, ?4)", params![&item_id, &form.nome, &form.setor, &nome_normalizado])?;
        tx.execute("INSERT INTO exemplares (numero_identificacao, item_id, status) VALUES (?1, ?2, 'Disponivel')", params![&form.numero_identificacao, &item_id])?;
        Ok(tx.commit())
    }).await.unwrap();
    Redirect::to("/cautela/catalogo").into_response()
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
        
        // Verificação para garantir que o item estava realmente disponível
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
        // Mensagem de erro mais específica
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
    /// Uma estrutura simples apenas para o teste.
    #[derive(serde::Serialize)]
    struct MensagemTeste {
        status: String,
        mensagem: String,
    }

    let payload = MensagemTeste {
        status: "OK".to_string(),
        mensagem: "Se vir este JSON, a configuração base está correta.".to_string(),
    };

    // Esta linha é o ponto central do teste.
    // Se esta linha falhar, o problema está nas dependências ou na configuração do projeto.
    (StatusCode::OK, Json(payload)).into_response()
}