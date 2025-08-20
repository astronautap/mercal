// src/users.rs

use crate::auth::User;
use std::collections::HashMap;
use tokio::fs;
use crate::escala::Genero;

const USERS_FILE: &str = "users.json";
type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn ensure_users_file() {
    if fs::try_exists(USERS_FILE).await.unwrap_or(false) {
        return;
    }
    println!("📝 Ficheiro {} não encontrado. A criar um novo...", USERS_FILE);
    if let Err(e) = create_default_users_file().await {
        eprintln!("🔥 Falha crítica ao criar o ficheiro de utilizadores: {}", e);
    }
}

/// Cria o ficheiro `users.json` com utilizadores padrão, incluindo funções.
async fn create_default_users_file() -> AppResult<()> {
    let cost = bcrypt::DEFAULT_COST;
    let default_users = vec![
        User {
            id: "1000".to_string(),
            password: bcrypt::hash("1234", cost)?,
            name: "Administrador".to_string(),
            turma: "T100".to_string(),
            ano: 1,
            curso: 'B',
            genero: Genero::Masculino,
            // Atribui a função 'admin'
            roles: vec!["admin".to_string()],
        },
        User {
            id: "1001".to_string(),
            password: bcrypt::hash("1234", cost)?,
            name: "Chefe".to_string(),
            turma: "T100".to_string(),
            ano: 2,
            curso: 'N',
            genero: Genero::Masculino,
            // Atribui a função 'rancheiro'
            roles: vec!["rancheiro".to_string()],
        },
        User {
            id: "1002".to_string(),
            password: bcrypt::hash("1234", cost)?,
            name: "Um".to_string(),
            turma: "T100".to_string(),
            ano: 3,
            curso: 'M',
            genero: Genero::Feminino,
            // Utilizador comum, sem funções especiais
            roles: vec![],
        },
    ];
    let json_content = serde_json::to_string_pretty(&default_users)?;
    fs::write(USERS_FILE, json_content).await?;
    println!("✅ Ficheiro {} criado com sucesso.", USERS_FILE);
    Ok(())
}

pub async fn load_users() -> AppResult<HashMap<String, User>> {
    let content = fs::read_to_string(USERS_FILE).await?;
    let users_vec: Vec<User> = serde_json::from_str(&content)?;
    let users_map = users_vec.into_iter().map(|user| (user.id.clone(), user)).collect();
    Ok(users_map)
}

pub async fn save_users(users: &HashMap<String, User>) -> AppResult<()> {
    let users_vec: Vec<User> = users.values().cloned().collect();
    let json_content = serde_json::to_string_pretty(&users_vec)?;
    fs::write(USERS_FILE, json_content).await?;
    Ok(())
}