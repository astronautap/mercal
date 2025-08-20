// src/main.rs

mod auth;
mod handlers;
mod presence;
mod presence_handlers;
mod users;
mod admin_handlers;
mod meals;
mod meals_handlers;
mod checkin;
mod checkin_handlers;
mod presence_state; 
mod escala;
mod escala_handlers;
mod escala_pdf;
mod escala_admin_handlers; 

use axum::{
    routing::{get, post},
    Router,
};
use std::{collections::HashSet, net::SocketAddr, sync::{Arc, Mutex}};
use tower_cookies::CookieManagerLayer;

#[tokio::main]
async fn main() {
    println!("🚀 A iniciar o servidor MercAl...");

    // Garante a criação de todos os ficheiros e pastas necessários
    users::ensure_users_file().await;
    presence::ensure_presence_file().await;
    meals::ensure_meals_structure().await;
    escala::ensure_escala_structure().await;

    let users_map = users::load_users().await.unwrap();
    
    // Inicializa o estado da aplicação
    let app_state = auth::AppState {
        sessions: Arc::new(Mutex::new(HashSet::new())),
        users: Arc::new(Mutex::new(users_map)),
        checkin_state: checkin::CheckinState::default(),
        presence_state: presence_state::PresenceSocketState::default(),
    };

    // Define todas as rotas da aplicação
    let app = Router::new()
        // Rotas Principais e de Autenticação
        .route("/", get(handlers::login_page))
        .route("/login", post(handlers::login_handler))
        .route("/dashboard", get(handlers::dashboard_handler))
        .route("/logout", get(handlers::logout_handler))
        .route("/admin", get(admin_handlers::admin_page_handler))
        .route("/admin/change-password", post(admin_handlers::change_password_handler))
        .route("/admin/create-user", post(admin_handlers::create_user_handler))
        
        // Rotas de Presença
        .route("/presence", get(presence_handlers::presence_page))
        .route("/ws/presence", get(presence_handlers::presence_websocket_handler))
        
        // Rotas de Refeições
        .route("/refeicoes", get(meals_handlers::user_meals_page))
        .route("/refeicoes/save_all", post(meals_handlers::save_all_meals_handler))
        .route("/admin/refeicoes", get(meals_handlers::admin_meals_page))
        .route("/admin/refeicoes/open", post(meals_handlers::open_meals_form))
        .route("/admin/refeicoes/close", post(meals_handlers::close_meals_form))
        .route("/admin/refeicoes/reopen", post(meals_handlers::reopen_active_period_form))
        .route("/admin/refeicoes/save_edits", post(meals_handlers::save_edits_form))
        
        // Rotas de Check-in de Refeições
        .route("/refeicoes/checkin", get(checkin_handlers::checkin_page))
        .route("/ws/refeicoes/checkin", get(checkin_handlers::checkin_websocket_handler))
        .route("/refeicoes/checkin/relatorio_ausentes", get(checkin_handlers::generate_absent_report_handler))
        
        // --- ROTAS DO MÓDULO DE ESCALAS (REORGANIZADAS) ---
        // Rotas de Utilizador
        .route("/escala", get(escala_handlers::user_escala_page))
        .route("/escala/pedir_troca", post(escala_handlers::pedir_troca_handler))
        .route("/escala/responder_troca", post(escala_handlers::responder_troca_handler))

        // Rotas de Administração
        .route("/admin/escala", get(escala_admin_handlers::admin_escala_page))
        .route("/admin/escala/gerar", post(escala_admin_handlers::gerar_escala_handler))
        .route("/admin/escala/lancar", post(escala_admin_handlers::lancar_escala_handler))
        .route("/admin/escala/aprovar_troca", post(escala_admin_handlers::aprovar_troca_handler))
        .route("/admin/escala/fechar_trocas", post(escala_admin_handlers::fechar_trocas_handler))
        .route("/admin/escala/reabrir_trocas", post(escala_admin_handlers::reabrir_trocas_handler))
        .route("/admin/escala/indisponibilidade/adicionar", post(escala_admin_handlers::adicionar_indisponibilidade_handler))
        .route("/admin/escala/indisponibilidade/remover", post(escala_admin_handlers::remover_indisponibilidade_handler))
        // --- NOVAS ROTAS DE PUNIÇÃO ---
        .route("/admin/escala/punicao/adicionar", post(escala_admin_handlers::adicionar_punicao_handler))
        .route("/admin/escala/punicao/remover", post(escala_admin_handlers::remover_punicao_handler))
        .route("/admin/escala/configuracao/salvar", post(escala_admin_handlers::salvar_configuracao_punicao_handler))
        .route("/admin/escala/pdf", get(escala_admin_handlers::gerar_pdf_escala_handler))
        .route("/admin/escala/troca_obrigatoria", post(escala_admin_handlers::troca_obrigatoria_handler))

        .with_state(app_state)
        .layer(CookieManagerLayer::new());
    
    //172.20.10.3

    let addr = SocketAddr::from(([172, 20, 10, 3], 3000));
    println!("✅ Servidor a escutar em http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();
}
