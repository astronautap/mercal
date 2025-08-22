// src/views/cautela.rs

use crate::auth::User;
use crate::cautela::{AtrasoInfo, Emprestimo, ItemCatalogo, StatusExemplar};
use axum::response::Html;
use chrono::Local;
use std::collections::HashMap;

// O conteúdo do `mod view` antigo vem para aqui.
// Todas as funções são marcadas com `pub`.

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

pub fn catalogo_page(
    catalogo: &[ItemCatalogo], 
    setores: &[String], 
    selected_setor: Option<&str>,
    active_loans: &HashMap<String, Emprestimo>,
    users: &HashMap<String, User>,
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
                event.preventDefault();

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

                if (emprestimo) {{
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
                }} else {{
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