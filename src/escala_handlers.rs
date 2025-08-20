// src/escala_handlers.rs

use crate::auth::{self, AppState, User};
use crate::escala::{Alocacao, EstadoEscala, EscalaDiaria, Posto, TipoServico, DetalheServico, TipoTroca, StatusTroca, Troca};
use axum::{
    debug_handler,
    extract::{State, Form},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use chrono::{NaiveDate, Duration, Datelike, Weekday};
use std::collections::{HashMap, BTreeMap, HashSet};
use tokio::fs;
use tower_cookies::Cookies;
use uuid::Uuid;

// Constantes usadas pelos handlers de utilizador
const ESTADO_ESCALA_FILE: &str = "data/escala/estado.json";
const POSTOS_FILE: &str = "data/escala/postos.json";
const ESCALA_DATA_DIR: &str = "data/escala";
const TROCAS_FILE: &str = "data/escala/trocas.json";
const USERS_FILE: &str = "users.json";

// --- MÓDULO DE VISUALIZAÇÃO (HTML e CSS) ---
mod view {
    use axum::response::Html;

    // 1. CSS CENTRALIZADO E COM ESTILO MATERIAL DESIGN
    const CSS: &str = r#"
        :root {
            --primary-color: #3f51b5; /* Indigo */
            --primary-dark: #303f9f;
            --accent-color: #ff4081; /* Pink */
            --background-color: #f5f5f5;
            --card-background: #ffffff;
            --text-color: #212121;
            --text-light: #757575;
            --border-color: #e0e0e0;
            --shadow: 0 2px 4px rgba(0,0,0,0.1), 0 2px 10px rgba(0,0,0,0.08);
            --success-color: #4caf50;
            --danger-color: #f44336;
        }
        body {
            font-family: 'Roboto', -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
            background-color: var(--background-color);
            color: var(--text-color);
            margin: 0;
            line-height: 1.6;
        }
        .container { max-width: 1200px; margin: 20px auto; padding: 0 15px; }
        .card {
            background-color: var(--card-background);
            border-radius: 8px;
            box-shadow: var(--shadow);
            padding: 24px;
            margin-bottom: 25px;
        }
        h1 { text-align: center; color: var(--primary-dark); }
        a { color: var(--primary-color); text-decoration: none; }
        
        /* Estilos dos Tabs */
        .tab-container { display: flex; justify-content: space-between; align-items: center; background-color: var(--card-background); border-radius: 8px; padding: 8px; box-shadow: var(--shadow); margin-bottom: 25px; }
        .tab-buttons { display: flex; gap: 8px; }
        .tab-btn {
            padding: 10px 20px; border: none; border-radius: 6px;
            background-color: transparent; color: var(--text-light);
            font-size: 16px; font-weight: 500; cursor: pointer; transition: all 0.3s;
        }
        .tab-btn.active { background-color: var(--primary-color); color: white; }
        .tab-link { font-weight: 500; padding: 10px 20px; }
        .tabcontent { display: none; }

        /* Estilos da Escala */
        .day-card {
            background-color: var(--card-background);
            border-radius: 8px;
            padding: 20px;
            margin-bottom: 40px;
            box-shadow: var(--shadow);
        }
        .day-card h2 { margin-top: 0; border-bottom: 1px solid var(--border-color); padding-bottom: 10px; font-size: 1.2em; color: var(--primary-dark); }
        table { width: 100%; border-collapse: collapse; margin-top: 15px; }
        th, td { border: 1px solid var(--border-color); text-align: center; font-size: 0.85em; }
        th { background-color: #f8f9fa; font-weight: 500; }
        td.person-cell { cursor: pointer; font-weight: 500; }
        td.person-cell:hover { background-color: #e8eaf6; }
        .meu-servico { background-color:rgb(164, 254, 167); font-weight: bold; padding: 4px; border-radius: 4px; }
        .punicao-cell { background-color: #ffcdd2; color: #b71c1c; cursor: not-allowed; }
        .section-header { color: black; padding: 2px; text-align: center; font-weight: 500; margin-top: 20px; border-radius: 4px; font-size: 0.9em; }
        .empty-cell { border: none; background: transparent; }

        /* Estilos do Modal */
        .modal { display: none; position: fixed; z-index: 1000; left: 0; top: 0; width: 100%; height: 100%; overflow: auto; background-color: rgba(0,0,0,0.5); }
        .modal-content {
            background-color: var(--card-background); margin: 10% auto; padding: 25px;
            border: none; width: 90%; max-width: 500px; border-radius: 8px; box-shadow: 0 5px 15px rgba(0,0,0,0.3);
        }
        .close-button { color: var(--text-light); float: right; font-size: 28px; font-weight: bold; cursor: pointer; }
    "#;

    // 2. FUNÇÃO DE LAYOUT
    fn render_page(title: &str, content: String) -> Html<String> {
        Html(format!(
            r#"
            <!DOCTYPE html>
            <html lang="pt-BR">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>{title}</title>
                <link rel="preconnect" href="https://fonts.googleapis.com">
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
                <link href="https://fonts.googleapis.com/css2?family=Roboto:wght@400;500;700&display=swap" rel="stylesheet">
                <style>{CSS}</style>
            </head>
            <body>
                <div class="container">
                    {content}
                </div>
            </body>
            </html>
            "#,
            title = title,
            CSS = CSS,
            content = content
        ))
    }

    // 3. FUNÇÃO DE RENDERIZAÇÃO DA PÁGINA DE ESCALA
    pub fn render_escala_page(
        is_admin: bool,
        html_escala_atual: &str,
        html_escala_seguinte: &str,
        escala_json_for_script: &str,
        users_json_for_script: &str,
        status_trocas: &str,
        user_id: &str,
    ) -> Html<String> {
        let content = format!(
            r#"
            <h1>📅 Consultar Escalas</h1>

            <div class="tab-container">
                <div class="tab-buttons">
                    <button class="tab-btn active" onclick="openTab(event, 'Atual')" id="defaultOpen">Escala Atual</button>
                    <button class="tab-btn" onclick="openTab(event, 'Proxima')">Próxima Escala</button>
                </div>
                <a href="/dashboard" class="tab-link">← Voltar ao Dashboard</a>
            </div>

            <div id="Atual" class="tabcontent" style="display: block;">
                <h2>Escala em Vigor ({escala_atual_subtitulo})</h2>
                {html_escala_atual}
            </div>

            <div id="Proxima" class="tabcontent">
                <h2>Próxima Escala (Período de Trocas)</h2>
                {html_escala_seguinte}
            </div>

            <!-- Modals e Scripts -->
            <div id="tradeModal" class="modal">
              <div class="modal-content">
                <span class="close-button" onclick="closeAllModals()">&times;</span>
                <h2 id="modal_title"></h2>
                <form action="/escala/pedir_troca" method="post">
                    <input type="hidden" id="requester_service_json" name="requester_service_json">
                    <input type="hidden" id="target_service_json" name="target_service_json">
                    <input type="hidden" id="tipo_troca" name="tipo_troca">
                    <p><strong>Serviço Alvo:</strong> <span id="target_service_text"></span></p>
                    <p><strong>Seu Serviço Envolvido:</strong> <span id="requester_service_text"></span></p>
                    <label for="motivo">Motivo do Pedido:</label>
                    <textarea name="motivo" required style="width: 100%; height: 60px;"></textarea>
                    <br><br>
                    <button type="submit" style="padding: 10px 15px;">Confirmar e Enviar Pedido</button>
                </form>
              </div>
            </div>

            <div id="adminTradeModal" class="modal">
              <div class="modal-content">
                <span class="close-button" onclick="closeAllModals()">&times;</span>
                <h2>Troca Obrigatória</h2>
                <form action="/admin/escala/troca_obrigatoria" method="post" onsubmit="return confirm('Tem a certeza que deseja efetuar esta troca obrigatória?');">
                    <input type="hidden" id="admin_original_service_json" name="original_service_json">
                    <p><strong>A substituir:</strong> <span id="admin_target_service_text"></span></p>
                    <hr>
                    <label for="substitute_user_id"><strong>ID do Substituto:</strong></label>
                    <input type="text" id="substitute_user_id" name="substitute_user_id" list="user-list" required style="width:100%; padding: 8px; margin-top: 8px;">
                    <datalist id="user-list"></datalist>
                    <br><br>
                    <button type="submit" style="padding: 10px 15px; background-color: #dc3545; color: white; border: none;">Efetuar Troca</button>
                </form>
              </div>
            </div>

            <script id="escala-data" type="application/json">{escala_json_for_script}</script>
            <script id="users-data" type="application/json">{users_json_for_script}</script>
            <script>
                function openTab(evt, tabName) {{
                    document.querySelectorAll(".tabcontent").forEach(tc => tc.style.display = "none");
                    document.querySelectorAll(".tab-btn").forEach(tb => tb.classList.remove("active"));
                    document.getElementById(tabName).style.display = "block";
                    evt.currentTarget.classList.add("active");
                }}

                const allModals = document.querySelectorAll('.modal');
                const escalaData = JSON.parse(document.getElementById('escala-data').textContent);
                const allUsers = JSON.parse(document.getElementById('users-data').textContent);
                const loggedInUserId = "{user_id}";
                const periodoDeTrocasStatus = "{status_trocas}";

                const userList = document.getElementById('user-list');
                allUsers.forEach(user => {{
                    const option = document.createElement('option');
                    option.value = user.id;
                    option.innerText = `${{user.name}} (${{user.id}})`;
                    userList.appendChild(option);
                }});

                function closeAllModals() {{ allModals.forEach(m => m.style.display = "none"); }}
                window.onclick = function(event) {{ if (event.target.classList.contains('modal')) {{ closeAllModals(); }} }}

                function findUserService() {{
                    for (const [date, dailyScale] of Object.entries(escalaData)) {{
                        for (const [posto, horarios] of Object.entries(dailyScale.escala)) {{
                            for (const [horario, alocacao] of Object.entries(horarios)) {{
                                if (alocacao.user_id === loggedInUserId) {{ return {{ data: date, posto, horario, user_id: loggedInUserId }}; }}
                            }}
                        }}
                        if (dailyScale.retem) {{
                            for (const alocacao of dailyScale.retem) {{
                                if (alocacao.user_id === loggedInUserId) {{ return {{ data: date, posto: 'RETEM', horario: 'SOBREAVISO', user_id: loggedInUserId }}; }}
                            }}
                        }}
                    }}
                    return null;
                }}

                function openTradeModal(cell) {{
                    if (cell.getAttribute('data-punicao') === 'true') {{ alert('Não é possível solicitar troca com um serviço de punição.'); return; }}
                    if (periodoDeTrocasStatus !== 'Aberto') {{ alert('O período para solicitar trocas está fechado.'); return; }}
                    
                    const targetServiceDetails = {{ data: cell.getAttribute('data-date'), posto: cell.getAttribute('data-posto'), horario: cell.getAttribute('data-horario'), user_id: cell.getAttribute('data-alvo-id'), }};
                    if(targetServiceDetails.user_id === loggedInUserId) return;

                    const targetDisplayName = cell.getAttribute('data-alvo-nome');
                    const requesterService = findUserService();
                    
                    document.getElementById('target_service_json').value = JSON.stringify(targetServiceDetails);
                    document.getElementById('target_service_text').innerText = `${{targetServiceDetails.posto}} em ${{new Date(targetServiceDetails.data + 'T00:00:00').toLocaleDateString('pt-BR',{{timeZone: 'UTC'}})}} (${{targetDisplayName}})`;

                    let tipoTroca = 'Cobertura';
                    if (requesterService) {{
                        if ((targetServiceDetails.posto === 'RETEM') === (requesterService.posto === 'RETEM')) {{ tipoTroca = 'Permuta'; }}
                    }}

                    if (tipoTroca === 'Permuta') {{
                        document.getElementById('modal_title').innerText = 'Solicitar Permuta de Serviço';
                        document.getElementById('requester_service_text').innerText = `${{requesterService.posto}} em ${{new Date(requesterService.data + 'T00:00:00').toLocaleDateString('pt-BR',{{timeZone: 'UTC'}}) }}`;
                        document.getElementById('requester_service_json').value = JSON.stringify(requesterService);
                    }} else {{
                        document.getElementById('modal_title').innerText = 'Solicitar Cobertura de Serviço';
                        document.getElementById('requester_service_text').innerText = requesterService ? `Seu serviço (${{requesterService.posto}}) não é compatível para permuta.` : 'Você está de folga e irá cobrir este serviço.';
                        document.getElementById('requester_service_json').value = JSON.stringify(requesterService || {{ user_id: loggedInUserId }});
                    }}
                    document.getElementById('tipo_troca').value = tipoTroca;
                    document.getElementById('tradeModal').style.display = "block";
                }}
                
                function openAdminTradeModal(cell) {{
                    const originalServiceDetails = {{ data: cell.getAttribute('data-date'), posto: cell.getAttribute('data-posto'), horario: cell.getAttribute('data-horario'), user_id: cell.getAttribute('data-alvo-id'), }};
                    const originalUserName = cell.getAttribute('data-alvo-nome');

                    document.getElementById('admin_original_service_json').value = JSON.stringify(originalServiceDetails);
                    document.getElementById('admin_target_service_text').innerText = `${{originalUserName}} (${{originalServiceDetails.posto}} em ${{new Date(originalServiceDetails.data + 'T00:00:00').toLocaleDateString('pt-BR',{{timeZone: 'UTC'}})}})`;
                    document.getElementById('substitute_user_id').value = '';
                    document.getElementById('adminTradeModal').style.display = "block";
                }}
            </script>
            "#,
            escala_atual_subtitulo = if is_admin { "Modo TO" } else { "Apenas Consulta" },
            html_escala_atual = html_escala_atual,
            html_escala_seguinte = html_escala_seguinte,
            escala_json_for_script = escala_json_for_script,
            users_json_for_script = users_json_for_script,
            status_trocas = status_trocas,
            user_id = user_id
        );
        render_page("Consultar Escala", content)
    }
}


// --- FUNÇÕES AUXILIARES PARA FORMATAÇÃO ---

fn formatar_dia_semana_completo(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Segunda-Feira",
        Weekday::Tue => "Terça-Feira",
        Weekday::Wed => "Quarta-Feira",
        Weekday::Thu => "Quinta-Feira",
        Weekday::Fri => "Sexta-Feira",
        Weekday::Sat => "Sábado",
        Weekday::Sun => "Domingo",
    }
}

fn formatar_tipo_servico(tipo: &TipoServico) -> &'static str {
    match tipo {
        TipoServico::RN => "Rotina Normal",
        TipoServico::RD | TipoServico::UDRD | TipoServico::ER => "Rotina de Domingo",
        TipoServico::Retem => "Retém",
    }
}


/// Gera o HTML para uma tabela de escala de um período específico.
async fn gerar_html_escala_inner(
    periodo: &crate::escala::Periodo,
    _postos: &[crate::escala::Posto],
    users: &HashMap<String, User>,
    user_id: &str,
    interativo_proxima: bool,
    interativo_atual_admin: bool,
) -> (String, BTreeMap<NaiveDate, EscalaDiaria>) {
    let mut html_output = String::new();
    let mut escalas_map = BTreeMap::new();
    
    let mut current_date = periodo.start_date;
    while current_date <= periodo.end_date {
        let filename = format!("{}/{}.json", ESCALA_DATA_DIR, current_date.format("%Y-%m-%d"));
        if let Ok(content) = fs::read_to_string(&filename).await {
            if let Ok(escala_diaria) = serde_json::from_str::<EscalaDiaria>(&content) {
                
                let dia_semana_formatado = formatar_dia_semana_completo(current_date.weekday());
                let data_formatada = current_date.format("%d/%m/%Y");
                let tipo_servico_formatado = formatar_tipo_servico(&escala_diaria.tipo_dia);
                let titulo_dia = format!("Detalhe de {}, {}, {}", dia_semana_formatado, data_formatada, tipo_servico_formatado);

                let mut daily_html = String::new();

                let seccoes = vec![
                    ("3º ANO", vec!["AJOSCA", "RANCHEIRO", "CHEFE DE DIA"]),
                    ("2º ANO", vec!["SALÃO DE VÍDEO", "SALÃO DE RECREIO", "LOJA SAMM", "SUBCHEFE", "CONFERÊNCIA", "GARAGEM", "POLÍCIA", "COPA"]),
                    ("1º ANO", vec!["ENTREGADOR", "PAV 3A", "PAV 3B", "PAV 2", "GUARDA PAV FEM", "RONDA", "CLAVICULÁRIO"]),
                    ("PAV FEM", vec!["PAV 2 - FEM", "LAVANDERIA"]),
                ];

                for (titulo_seccao, nomes_postos) in seccoes {
                    let mut postos_diario = Vec::new();
                    let mut postos_turnos = Vec::new();
                    let mut horarios_seccao = HashSet::new();

                    for nome_posto in &nomes_postos {
                        if let Some(horarios_do_posto) = escala_diaria.escala.get(*nome_posto) {
                            if horarios_do_posto.contains_key("DIARIO") { postos_diario.push(nome_posto); } else { postos_turnos.push(nome_posto); for horario in horarios_do_posto.keys() { horarios_seccao.insert(horario.clone()); } }
                        }
                    }

                    if postos_diario.is_empty() && postos_turnos.is_empty() { continue; }

                    daily_html.push_str(&format!("<div class='section-header'>{}</div>", titulo_seccao));

                    if !postos_diario.is_empty() {
                        daily_html.push_str("<table><thead><tr><th>Posto</th><th>Serviço</th><th>Posto</th><th>Serviço</th><th>Posto</th><th>Serviço</th></tr></thead><tbody>");
                        let mut i = 0;
                        while i < postos_diario.len() {
                            daily_html.push_str("<tr>");
                            for j in 0..3 {
                                if i + j < postos_diario.len() {
                                    let nome_posto = postos_diario[i + j];
                                    if let Some(alocacao) = escala_diaria.escala.get(*nome_posto).and_then(|h| h.get("DIARIO")) {
                                        let (cell_class, on_click_attr) = if interativo_atual_admin {
                                            ("person-cell".to_string(), format!("onclick=\"openAdminTradeModal(this)\""))
                                        } else if interativo_proxima && !alocacao.punicao {
                                            ("person-cell".to_string(), format!("onclick=\"openTradeModal(this)\""))
                                        } else if alocacao.punicao {
                                            ("punicao-cell".to_string(), "".to_string())
                                        } else {
                                            ("".to_string(), "".to_string())
                                        };

                                        let display_name = users.get(&alocacao.user_id).map(|u| format!("{}{}", u.curso, u.id)).map(|p| format!("{} {}", p, &alocacao.nome)).unwrap_or_else(|| alocacao.nome.clone());
                                        let cell_content = if alocacao.user_id == user_id { format!("<div class='meu-servico'>{}</div>", display_name) } else { display_name };
                                        
                                        daily_html.push_str(&format!( "<td><strong>{}</strong></td><td class='{_class}' data-date='{_date}' data-posto='{_posto}' data-horario='DIARIO' data-alvo-id='{_id}' data-alvo-nome='{_nome}' data-punicao='{_punicao}' {_onclick}>{_content}</td>", nome_posto, _class = cell_class.trim(), _date = current_date, _posto = nome_posto, _id = alocacao.user_id, _nome = alocacao.nome.clone(), _punicao = alocacao.punicao, _onclick = on_click_attr, _content = cell_content ));
                                    } else { daily_html.push_str(&format!("<td><strong>{}</strong></td><td>---</td>", nome_posto)); }
                                } else { daily_html.push_str(r#"<td class="empty-cell"></td><td class="empty-cell"></td>"#); }
                            }
                            daily_html.push_str("</tr>");
                            i += 3;
                        }
                        daily_html.push_str("</tbody></table>");
                    }

                    if !postos_turnos.is_empty() {
                        let mut horarios_ordenados: Vec<String> = horarios_seccao.into_iter().collect();
                        horarios_ordenados.sort();
                        let mut table_header = String::from("<th>Posto</th>");
                        for horario in &horarios_ordenados { table_header.push_str(&format!("<th>{}</th>", horario.replace("/", "<br>"))); }
                        daily_html.push_str(&format!("<table><thead><tr>{}</tr></thead><tbody>", table_header));
                        for nome_posto in postos_turnos {
                            daily_html.push_str(&format!("<tr><td><strong>{}</strong></td>", nome_posto));
                            for horario in &horarios_ordenados {
                                if let Some(alocacao) = escala_diaria.escala.get(*nome_posto).and_then(|h| h.get(horario)) {
                                    let (cell_class, on_click_attr) = if interativo_atual_admin {
                                        ("person-cell".to_string(), format!("onclick=\"openAdminTradeModal(this)\""))
                                    } else if interativo_proxima && !alocacao.punicao {
                                        ("person-cell".to_string(), format!("onclick=\"openTradeModal(this)\""))
                                    } else if alocacao.punicao {
                                        ("punicao-cell".to_string(), "".to_string())
                                    } else {
                                        ("".to_string(), "".to_string())
                                    };
                                    
                                    let display_name = users.get(&alocacao.user_id).map(|u| format!("{}{}", u.curso, u.id)).map(|p| format!("{} {}", p, &alocacao.nome)).unwrap_or_else(|| alocacao.nome.clone());
                                    let cell_content = if alocacao.user_id == user_id { format!("<div class='meu-servico'>{}</div>", display_name) } else { display_name };
                                    
                                    daily_html.push_str(&format!("<td class='{_class}' data-date='{_date}' data-posto='{_posto}' data-horario='{_horario}' data-alvo-id='{_id}' data-alvo-nome='{_nome}' data-punicao='{_punicao}' {_onclick}>{_content}</td>", _class = cell_class.trim(), _date = current_date, _posto = nome_posto, _horario = horario, _id = alocacao.user_id, _nome = alocacao.nome, _punicao = alocacao.punicao, _onclick = on_click_attr, _content = cell_content));
                                } else { daily_html.push_str("<td>---</td>"); }
                            }
                            daily_html.push_str("</tr>");
                        }
                        daily_html.push_str("</tbody></table>");
                    }
                }
                
                if !escala_diaria.retem.is_empty() {
                    let mut retem_por_ano: BTreeMap<u8, Vec<&Alocacao>> = BTreeMap::new();
                    for alocacao in &escala_diaria.retem { if let Some(user) = users.get(&alocacao.user_id) { retem_por_ano.entry(user.ano).or_default().push(alocacao); } }
                    
                    daily_html.push_str("<div class='section-header'>EQUIPE DE RETÉM</div>");
                    daily_html.push_str("<table><thead><tr><th colspan='8'>Membros de Sobreaviso</th></tr></thead><tbody>");

                    let mut all_retem_alocacoes: Vec<&Alocacao> = Vec::new();
                    for ano_num in [3, 2, 1] { if let Some(alocacoes) = retem_por_ano.get(&ano_num) { all_retem_alocacoes.extend(alocacoes.iter()); } }

                    let mut i = 0;
                    while i < all_retem_alocacoes.len() {
                        daily_html.push_str("<tr>");
                        for j in 0..4 {
                            if i + j < all_retem_alocacoes.len() {
                                let alocacao = all_retem_alocacoes[i + j];
                                let (cell_class, on_click_attr) = if interativo_atual_admin {
                                    ("person-cell".to_string(), format!("onclick=\"openAdminTradeModal(this)\""))
                                } else if interativo_proxima {
                                    ("person-cell".to_string(), format!("onclick=\"openTradeModal(this)\""))
                                } else {
                                    ("".to_string(), "".to_string())
                                };

                                let display_name = users.get(&alocacao.user_id).map(|u| format!("{}{} {}", u.curso, u.id, &alocacao.nome)).unwrap_or_else(|| alocacao.nome.clone());
                                let cell_content = if alocacao.user_id == user_id { format!("<div class='meu-servico'>{}</div>", display_name) } else { display_name };

                                daily_html.push_str(&format!("<td colspan='2' class='{_class}' data-date='{_date}' data-posto='RETEM' data-horario='SOBREAVISO' data-alvo-id='{_id}' data-alvo-nome='{_nome}' data-punicao='false' {_onclick}>{_content}</td>", _class = cell_class.trim(), _date = current_date, _id = alocacao.user_id, _nome = alocacao.nome, _onclick = on_click_attr, _content = cell_content));
                            } else { daily_html.push_str(r#"<td colspan='2' class="empty-cell"></td>"#); }
                        }
                        daily_html.push_str("</tr>");
                        i += 4;
                    }
                    daily_html.push_str("</tbody></table>");
                }
                
                html_output.push_str(&format!("<div class='day-card'><h2>{}</h2>{}</div>", titulo_dia, daily_html));
                escalas_map.insert(current_date, escala_diaria);
            }
        }
        current_date += Duration::days(1);
    }
    (html_output, escalas_map)
}

#[debug_handler]
pub async fn user_escala_page(
    State(state): State<AppState>,
    cookies: Cookies,
) -> impl IntoResponse {
    let user_id = cookies.get("user_id").map(|c| c.value().to_string());
    if user_id.is_none() {
        return (StatusCode::UNAUTHORIZED, Html("<h1>Não autorizado</h1>")).into_response();
    }
    let user_id = user_id.unwrap();
    let is_admin = auth::has_role(&state, &cookies, "admin").await;

    let estado_content = fs::read_to_string(ESTADO_ESCALA_FILE).await.unwrap_or_default();
    let estado: EstadoEscala = serde_json::from_str(&estado_content).unwrap();
    
    let postos_content = fs::read_to_string(POSTOS_FILE).await.unwrap_or_default();
    let postos: Vec<Posto> = serde_json::from_str(&postos_content).unwrap_or_default();
    
    let users_content = fs::read_to_string(USERS_FILE).await.unwrap_or_else(|_| "[]".to_string());
    let users_vec: Vec<User> = serde_json::from_str(&users_content).unwrap_or_default();
    let users: HashMap<String, User> = users_vec.iter().map(|u| (u.id.clone(), u.clone())).collect();

    let (html_escala_atual, mut escalas_completas) = gerar_html_escala_inner(&estado.periodo_atual, &postos, &users, &user_id, false, is_admin).await;

    let html_escala_seguinte = if let Some(periodo_seguinte) = &estado.periodo_seguinte {
        let (html, escalas_seguinte) = gerar_html_escala_inner(periodo_seguinte, &postos, &users, &user_id, true, false).await;
        escalas_completas.extend(escalas_seguinte);
        html
    } else {
        "<p>Nenhuma próxima escala foi gerada ainda.</p>".to_string()
    };
    
    let escala_json_for_script = serde_json::to_string(&escalas_completas).unwrap_or_else(|_| "{}".to_string());
    let users_json_for_script = serde_json::to_string(&users_vec).unwrap_or_else(|_| "[]".to_string());

    view::render_escala_page(
        is_admin,
        &html_escala_atual,
        &html_escala_seguinte,
        &escala_json_for_script,
        &users_json_for_script,
        &estado.status_trocas,
        &user_id,
    ).into_response()
}


#[debug_handler]
pub async fn pedir_troca_handler(
    State(_state): State<AppState>,
    cookies: Cookies,
    Form(form_data): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    if cookies.get("user_id").is_none() {
        return Redirect::to("/");
    }

    let tipo_troca_str = form_data.get("tipo_troca").unwrap();
    let motivo = form_data.get("motivo").unwrap().clone();
    let target_service: DetalheServico = serde_json::from_str(form_data.get("target_service_json").unwrap()).unwrap();
    
    let tipo = match tipo_troca_str.as_str() {
        "Permuta" => TipoTroca::Permuta,
        _ => TipoTroca::Cobertura,
    };

    let requerente_json_str = form_data.get("requester_service_json").unwrap();
    let requerente = if tipo == TipoTroca::Permuta {
        serde_json::from_str(requerente_json_str).unwrap()
    } else {
        let user_info: HashMap<String, String> = serde_json::from_str(requerente_json_str).unwrap();
        DetalheServico {
            data: NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            posto: "FOLGA".to_string(),
            horario: "".to_string(),
            user_id: user_info.get("user_id").unwrap().clone(),
        }
    };
    
    let nova_troca = Troca {
        id: Uuid::new_v4().to_string(),
        tipo,
        requerente,
        alvo: target_service,
        motivo,
        status: StatusTroca::PendenteAlvo,
    };

    let mut trocas: Vec<Troca> = fs::read_to_string(TROCAS_FILE)
        .await
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default();
        
    trocas.push(nova_troca);
    
    if let Ok(json) = serde_json::to_string_pretty(&trocas) {
        let _ = fs::write(TROCAS_FILE, json).await;
    }

    Redirect::to("/escala")
}

#[debug_handler]
pub async fn responder_troca_handler(
    cookies: Cookies,
    Form(form_data): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let user_id = match cookies.get("user_id") {
        Some(cookie) => cookie.value().to_string(),
        None => return Redirect::to("/"),
    };

    let troca_id = form_data.get("troca_id").unwrap();
    let acao = form_data.get("acao").unwrap();

    let mut trocas: Vec<Troca> = fs::read_to_string(TROCAS_FILE).await
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();

    if let Some(troca) = trocas.iter_mut().find(|t| &t.id == troca_id) {
        if troca.alvo.user_id != user_id {
            return Redirect::to("/dashboard");
        }
        if troca.status != StatusTroca::PendenteAlvo {
            return Redirect::to("/dashboard");
        }

        troca.status = if acao == "aprovar" {
            StatusTroca::PendenteAdmin
        } else {
            StatusTroca::Recusada
        };
    }

    if let Ok(json) = serde_json::to_string_pretty(&trocas) {
        let _ = fs::write(TROCAS_FILE, json).await;
    }

    Redirect::to("/dashboard")
}
