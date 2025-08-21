// src/cautela.rs

//! # Módulo para Gestão de Empréstimos (Cautela)
//!
//! Este módulo define todas as estruturas de dados e funções de acesso
//! ao banco de dados para o sistema de empréstimo e devolução de itens.

use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio_rusqlite::Connection;

// --- CONSTANTES DE DIRETÓRIO E BANCO DE DADOS ---
pub const PASTA_PAIOL: &str = "data/paioldelivros";
pub const DB_FILE: &str = "data/paioldelivros/paioldelivros.db";

// --- ESTRUTURAS DE DADOS (structs) ---
// Estas structs representam os dados que movemos de e para o banco de dados.

#[derive(Debug, Clone)]
pub struct Responsavel {
    pub username: String,
    pub password_hash: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum StatusExemplar {
    Disponivel,
    Emprestado,
}

#[derive(Debug, Clone)]
pub struct Exemplar {
    pub numero_identificacao: String,
    pub status: StatusExemplar,
}

#[derive(Debug, Clone)]
pub struct ItemCatalogo {
    pub id: String,
    pub nome: String,
    pub setor: String,
    pub exemplares: Vec<Exemplar>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum TipoEvento {
    Emprestimo,
    Renovacao,
    Devolucao,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EventoEmprestimo {
    pub tipo: TipoEvento,
    pub data_evento: DateTime<Local>,
    pub data_devolucao_prevista: NaiveDate,
    pub responsavel_id: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Emprestimo {
    pub id: String,
    pub item_id: String,
    pub exemplar_id: String,
    pub aluno_id: String,
    pub status: StatusExemplar,
    pub historico: Vec<EventoEmprestimo>,
}

// --- FUNÇÃO DE INICIALIZAÇÃO DO BANCO DE DADOS ---

/// Garante que a estrutura de diretórios e o banco de dados da cautela existam e estejam configurados.
pub async fn ensure_paioldelivros_structure() {
    if let Err(e) = fs::create_dir_all(PASTA_PAIOL).await {
        eprintln!("🔥 Falha crítica ao criar o diretório '{}': {}", PASTA_PAIOL, e);
        return;
    }

    if fs::try_exists(DB_FILE).await.unwrap_or(false) {
        return; // O banco de dados já existe, não faz nada.
    }
    
    println!("📝 A criar e inicializar o banco de dados em {}...", DB_FILE);
    match Connection::open(DB_FILE).await {
        Ok(conn) => {
            let _ = conn.call(|conn| {
                conn.execute_batch(
                    "BEGIN;
                    CREATE TABLE responsavel (
                        username TEXT PRIMARY KEY,
                        password_hash TEXT NOT NULL
                    );
                    CREATE TABLE itens (
                        id TEXT PRIMARY KEY,
                        nome TEXT NOT NULL,
                        setor TEXT NOT NULL,
                        nome_normalizado TEXT
                    );
                    CREATE TABLE exemplares (
                        numero_identificacao TEXT PRIMARY KEY,
                        item_id TEXT NOT NULL,
                        status TEXT NOT NULL,
                        FOREIGN KEY (item_id) REFERENCES itens (id)
                    );
                    CREATE TABLE emprestimos (
                        id TEXT PRIMARY KEY,
                        exemplar_id TEXT NOT NULL UNIQUE,
                        aluno_id TEXT NOT NULL,
                        status TEXT NOT NULL,
                        FOREIGN KEY (exemplar_id) REFERENCES exemplares (numero_identificacao)
                    );
                    CREATE TABLE historico_emprestimos (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        emprestimo_id TEXT NOT NULL,
                        tipo_evento TEXT NOT NULL,
                        data_evento TEXT NOT NULL,
                        data_devolucao_prevista TEXT NOT NULL,
                        responsavel_id TEXT NOT NULL,
                        FOREIGN KEY (emprestimo_id) REFERENCES emprestimos (id)
                    );
                    COMMIT;"
                )?;

                let default_pass = bcrypt::hash("12345", bcrypt::DEFAULT_COST).unwrap();
                conn.execute(
                    "INSERT INTO responsavel (username, password_hash) VALUES (?1, ?2)",
                    ("teste", default_pass),
                )?;
                Ok(())
            }).await;
            println!("✅ Banco de dados inicializado com sucesso.");
        }
        Err(e) => eprintln!("🔥 Falha crítica ao abrir/criar o banco de dados: {}", e),
    }
}