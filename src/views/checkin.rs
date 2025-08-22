// src/views/checkin.rs

use axum::response::{Html, IntoResponse};
use chrono::NaiveDate;

/// Apresenta a página de check-in de refeições.
pub fn checkin_page(
    today: NaiveDate,
    tab_buttons: String,
    tab_content: String,
) -> impl IntoResponse {
    if tab_content.is_empty() {
        return Html("<h1>Não foi possível carregar os dados das refeições para hoje.</h1><p>Verifique se o período de interesse foi aberto pelo rancheiro.</p><a href='/dashboard'>Voltar</a>").into_response();
    }

    Html(format!(
        r##"
        <!DOCTYPE html>
        <html lang="pt-BR">
        <head>
            <title>Check-in de Refeições</title>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                :root {{ --primary-color: #007bff; --secondary-color: #6c757d; --success-color: #28a745; --light-gray: #f8f9fa; --border-color: #dee2e6; }}
                body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; margin: 0; background-color: #f4f7f9; color: #333; }}
                .container {{ max-width: 1200px; margin: 0 auto; padding: 20px; }}
                .header-bar {{ background-color: white; padding: 15px 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); position: sticky; top: 0; z-index: 1000; }}
                .header-content {{ display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 15px; }}
                h1 {{ margin: 0; font-size: 24px; }}
                .search-bar {{ padding: 10px; font-size: 16px; border: 1px solid var(--border-color); border-radius: 6px; width: 100%; max-width: 400px; }}
                .tab-container {{ display: flex; border-bottom: 1px solid var(--border-color); background-color: var(--light-gray); }}
                .tablink {{ background-color: transparent; flex: 1; border: none; outline: none; cursor: pointer; padding: 14px 16px; transition: all 0.3s; font-size: 16px; font-weight: 500; border-bottom: 3px solid transparent; }}
                .tablink:hover {{ background-color: #e9ecef; }}
                .tablink.active {{ border-bottom-color: var(--primary-color); color: var(--primary-color); }}
                .tabcontent {{ display: none; padding: 20px; background-color: white; }}
                .tab-header {{ display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 10px; border-bottom: 1px solid var(--border-color); padding-bottom: 10px; margin-bottom: 20px; }}
                .tab-header h2 {{ margin: 0; font-size: 20px; }}
                .header-actions {{ display: flex; align-items: center; gap: 15px; }}
                .counter {{ font-size: 16px; font-weight: 500; background-color: var(--light-gray); padding: 5px 10px; border-radius: 6px; }}
                .report-btn {{ background-color: var(--primary-color); color: white; text-decoration: none; padding: 8px 12px; border-radius: 5px; font-size: 14px; font-weight: 500; }}
                .turma-header {{ color: var(--primary-color); margin-top: 20px; margin-bottom: 10px; border-bottom: 1px solid var(--border-color); padding-bottom: 5px; }}
                .user-list {{ list-style-type: none; padding: 0; }}
                .user-item {{ display: flex; justify-content: space-between; align-items: center; padding: 15px; border-bottom: 1px solid #f0f0f0; transition: background-color 0.2s; }}
                .user-item:last-child {{ border-bottom: none; }}
                .user-info {{ font-size: 16px; }}
                .status-display {{ display: flex; align-items: center; gap: 10px; }}
                .checkin-btn {{ padding: 8px 16px; cursor: pointer; background-color: var(--success-color); color: white; border: none; border-radius: 5px; font-weight: 500; }}
                .checkin-btn:disabled {{ background-color: var(--secondary-color); cursor: not-allowed; }}
                .marker-info {{ font-size: 12px; color: #555; }}
                .user-item.presente {{ background-color: #d4edda; color: #155724; }}
                .user-item.presente .user-info {{ text-decoration: line-through; }}
                .dashboard-link {{ display: inline-block; margin-top: 20px; color: var(--primary-color); text-decoration: none; font-weight: 500; }}
            </style>
        </head>
        <body>
            <div class="header-bar">
                <div class="container">
                    <div class="header-content">
                        <h1>Check-in de Refeições ({})</h1>
                        <input type="text" id="searchInput" class="search-bar" onkeyup="filterUsers()" placeholder="Pesquisar por número ou nome...">
                    </div>
                </div>
                <div class="tab-container">{}</div>
            </div>
            <div class="container">
                {}
                <a href="/dashboard" class="dashboard-link">← Voltar ao Dashboard</a>
            </div>

            <script>
                function openMeal(evt, mealName) {{
                    document.querySelectorAll(".tabcontent").forEach(tc => tc.style.display = "none");
                    document.querySelectorAll(".tablink").forEach(tl => tl.classList.remove("active"));
                    document.getElementById(mealName).style.display = "block";
                    evt.currentTarget.classList.add("active");
                    filterUsers();
                }}

                function filterUsers() {{
                    const input = document.getElementById("searchInput");
                    const filter = input.value.toLowerCase();
                    const activeTab = document.querySelector(".tabcontent[style*='block']");
                    if (!activeTab) return;

                    const items = activeTab.querySelectorAll(".user-item");
                    items.forEach(item => {{
                        const searchTerm = item.getAttribute('data-search-term');
                        if (searchTerm.includes(filter)) {{
                            item.style.display = "flex";
                        }} else {{
                            item.style.display = "none";
                        }}
                    }});
                }}

                const ws = new WebSocket(`ws://${{window.location.host}}/ws/refeicoes/checkin`);
                ws.onopen = () => console.log("WebSocket conectado.");
                ws.onmessage = function(event) {{
                    try {{
                        const update = JSON.parse(event.data);
                        const userRow = document.querySelector(`#${{update.meal}} .user-item[data-search-term*='${{update.user_id.toLowerCase()}}']`);
                        if (userRow && !userRow.classList.contains('presente')) {{
                            userRow.classList.add("presente");
                            const button = userRow.querySelector("button");
                            if (button) {{
                                button.disabled = true;
                            }}
                            
                            let statusDisplay = userRow.querySelector(".status-display");
                            if(statusDisplay){{
                                let markerSpan = statusDisplay.querySelector(".marker-info");
                                if(!markerSpan) {{
                                    markerSpan = document.createElement("span");
                                    markerSpan.className = "marker-info";
                                    statusDisplay.appendChild(markerSpan);
                                }}
                                markerSpan.textContent = `por ${{update.marked_by}} às ${{update.marked_at}}`;
                            }}

                            updateCounter(update.meal);
                        }}
                    }} catch (e) {{
                        console.error("Erro ao processar mensagem do servidor:", e);
                    }}
                }};

                function markPresent(userId, meal) {{
                    const action = {{ user_id: userId, meal: meal }};
                    ws.send(JSON.stringify(action));
                }}
                
                function updateCounter(meal) {{
                    const counterElement = document.getElementById(`counter-${{meal}}`);
                    if (!counterElement) return;
                    
                    let parts = counterElement.textContent.split('/');
                    let present = parseInt(parts[0].split(':')[1].trim(), 10);
                    let total = parseInt(parts[1].trim(), 10);
                    
                    present++;
                    counterElement.textContent = `Presentes: ${{present}} / ${{total}}`;
                }}

                document.addEventListener("DOMContentLoaded", () => {{
                    const firstTab = document.querySelector(".tablink");
                    if (firstTab) {{
                        firstTab.click();
                    }}
                }});
            </script>
        </body>
        </html>
    "##,
        today.format("%d/%m/%Y"),
        tab_buttons,
        tab_content
    ))
    .into_response()
}