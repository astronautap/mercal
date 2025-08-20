// src/escala.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::NaiveDate;
use tokio::fs;
use crate::auth::User;

// --- STRUCTS E ENUMS ---

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Genero {
    #[serde(rename = "M")]
    Masculino,
    #[serde(rename = "F")]
    Feminino,
    #[serde(rename = "X")]
    Misto,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum TipoServico {
    RN,
    RD,
    UDRD,
    ER,
    Retem,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Posto {
    pub nome: String,
    pub turmas_permitidas: Vec<u8>,
    pub genero: Genero,
    pub funcao_exclusiva: Option<String>,
    pub horarios_rn: Vec<String>,
    #[serde(default)]
    pub horarios_rd: Vec<String>,
    #[serde(default)]
    pub horarios_udrd: Vec<String>,
    #[serde(default)]
    pub horarios_er: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ContagemUtilizador {
    #[serde(default)]
    pub rn: u32,
    #[serde(default)]
    pub rd: u32,
    #[serde(default)]
    pub retem: u32,
}

pub type Contagem = HashMap<String, ContagemUtilizador>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Divida {
    pub credor: String,
    pub tipo_divida: TipoServico,
}
pub type DividasAtivas = HashMap<String, Vec<Divida>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Alocacao {
    pub user_id: String,
    pub nome: String,
    #[serde(default)]
    pub punicao: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EscalaDiaria {
    pub tipo_dia: TipoServico,
    pub escala: HashMap<String, HashMap<String, Alocacao>>,
    // --- CAMPO ADICIONADO PARA O RETÉM ---
    #[serde(default)]
    pub retem: Vec<Alocacao>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Periodo {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EstadoEscala {
    pub periodo_atual: Periodo,
    pub periodo_seguinte: Option<Periodo>,
    pub status_trocas: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DetalheServico {
    pub data: NaiveDate,
    pub posto: String,
    pub horario: String,
    pub user_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TipoTroca {
    Permuta,
    Cobertura,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum StatusTroca {
    PendenteAlvo,
    PendenteAdmin,
    Aprovada,
    Recusada,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Troca {
    pub id: String,
    pub tipo: TipoTroca,
    pub requerente: DetalheServico,
    pub alvo: DetalheServico,
    pub motivo: String,
    pub status: StatusTroca,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Indisponibilidade {
    pub user_id: String,
    pub data: NaiveDate,
    pub motivo: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Punicao {
    pub user_id: String,
    pub total_a_cumprir: u32,
    pub ja_cumpridos: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ConfiguracaoEscala {
    #[serde(default)]
    pub postos_punicao: Vec<String>,
}


// --- CONSTANTES DE FICHEIROS ---
pub const ESCALA_DATA_DIR: &str = "data/escala";
const ESTADO_FILE: &str = "data/escala/estado.json";
const POSTOS_FILE: &str = "data/escala/postos.json";
const CONTAGEM_FILE: &str = "data/escala/contagem.json";
const DIVIDAS_FILE: &str = "data/escala/dividas.json";
const TROCAS_FILE: &str = "data/escala/trocas.json";
const USERS_FILE: &str = "users.json";
const INDISPONIBILIDADE_FILE: &str = "data/escala/indisponibilidade.json";
const PUNIDOS_FILE: &str = "data/escala/punidos.json";
const CONFIGURACAO_FILE: &str = "data/escala/configuracao.json";


// --- LÓGICA PRINCIPAL DO ALGORITMO ---

pub async fn gerar_nova_escala(
    dias_da_escala: HashMap<NaiveDate, TipoServico>,
) -> Result<(), Box<dyn std::error::Error>> {
    
    fs::write(TROCAS_FILE, "[]").await?;

    // Carregamento de todos os dados necessários
    let todos_utilizadores: Vec<User> = serde_json::from_str(&fs::read_to_string(USERS_FILE).await?)?;
    let todos_postos: Vec<Posto> = serde_json::from_str(&fs::read_to_string(POSTOS_FILE).await?)?;
    let mut contagens: Contagem = serde_json::from_str(&fs::read_to_string(CONTAGEM_FILE).await?)?;
    let mut dividas: DividasAtivas = serde_json::from_str(&fs::read_to_string(DIVIDAS_FILE).await?)?;
    let mut punicoes: Vec<Punicao> = serde_json::from_str(&fs::read_to_string(PUNIDOS_FILE).await?)?;
    let config_escala: ConfiguracaoEscala = serde_json::from_str(&fs::read_to_string(CONFIGURACAO_FILE).await?)?;
    let indisponibilidades_content = fs::read_to_string(INDISPONIBILIDADE_FILE).await?;
    let todas_as_indisponibilidades: Vec<Indisponibilidade> = serde_json::from_str(&indisponibilidades_content)?;
    
    // Preparação das variáveis de estado do algoritmo
    let utilizadores_indisponiveis: Vec<String> = todas_as_indisponibilidades
        .into_iter()
        .map(|i| i.user_id)
        .collect();
    
    let mut dividas_pagas: Vec<(String, Divida)> = Vec::new();
    let mut utilizadores_fadigados: Vec<String> = Vec::new();

    let mut dias_ordenados: Vec<_> = dias_da_escala.into_iter().collect();
    dias_ordenados.sort_by_key(|k| k.0);

    // Lógica para adiar punições
    let mut dias_para_adiar = HashMap::new();
    for i in 0..dias_ordenados.len() {
        let is_dia_atual_especial = matches!(dias_ordenados[i].1, TipoServico::RD | TipoServico::UDRD | TipoServico::ER);
        if is_dia_atual_especial {
            let is_dia_anterior_especial = if i > 0 { matches!(dias_ordenados[i-1].1, TipoServico::RD | TipoServico::UDRD | TipoServico::ER) } else { false };
            let is_dia_seguinte_especial = if i + 1 < dias_ordenados.len() { matches!(dias_ordenados[i+1].1, TipoServico::RD | TipoServico::UDRD | TipoServico::ER) } else { false };
            if !is_dia_anterior_especial && is_dia_seguinte_especial {
                 dias_para_adiar.insert(dias_ordenados[i].0, true);
            }
        }
    }

    // Loop principal para gerar a escala de cada dia
    for (data, tipo_dia) in &dias_ordenados {
        let mut escala_do_dia: HashMap<String, HashMap<String, Alocacao>> = HashMap::new();
        let mut utilizadores_ja_alocados_hoje: Vec<String> = Vec::new();

        let mut exclusao_hoje = utilizadores_fadigados.clone();
        exclusao_hoje.extend(utilizadores_indisponiveis.clone());

        let (tipo_contagem, ordem_decrescente) = match tipo_dia {
            TipoServico::RN => (TipoServico::RN, false),
            _ => (TipoServico::RD, true),
        };

        // Geração da escala normal de postos
        for posto in &todos_postos {
            let mut escala_do_posto: HashMap<String, Alocacao> = HashMap::new();
            
            let mut candidatos_ao_posto: Vec<User> = todos_utilizadores.iter()
                .filter(|u| !exclusao_hoje.contains(&u.id))
                .filter(|u| posto.turmas_permitidas.contains(&u.ano))
                .filter(|u| match &posto.funcao_exclusiva {
                    Some(funcao) => u.roles.contains(funcao),
                    None => true,
                })
                .cloned().collect();

            candidatos_ao_posto.sort_by(|a, b| {
                let cont_a = contagens.get(&a.id).cloned().unwrap_or_default();
                let cont_b = contagens.get(&b.id).cloned().unwrap_or_default();
                let cont_val_a = if tipo_contagem == TipoServico::RN { cont_a.rn } else { cont_a.rd };
                let cont_val_b = if tipo_contagem == TipoServico::RN { cont_b.rn } else { cont_b.rd };
                cont_val_a.cmp(&cont_val_b).then_with(|| {
                    let num_a: u32 = a.id.chars().skip(1).collect::<String>().parse().unwrap_or(0);
                    let num_b: u32 = b.id.chars().skip(1).collect::<String>().parse().unwrap_or(0);
                    if ordem_decrescente { num_b.cmp(&num_a) } else { num_a.cmp(&num_b) }
                })
            });

            let horarios = match tipo_dia {
                TipoServico::RN => &posto.horarios_rn,
                TipoServico::RD => &posto.horarios_rd,
                TipoServico::UDRD => &posto.horarios_udrd,
                TipoServico::ER => &posto.horarios_er,
                _ => continue,
            };

            for horario in horarios {
                let mut alocacao_final: Option<Alocacao> = None;
                let mut id_para_contagem: Option<String> = None;

                // 1. TENTAR ALOCAR UM PUNIDO PRIMEIRO
                if matches!(tipo_dia, TipoServico::RD | TipoServico::UDRD | TipoServico::ER) 
                   && config_escala.postos_punicao.contains(&posto.nome) {
                    
                    let adiar_neste_dia = *dias_para_adiar.get(data).unwrap_or(&false);

                    for punido in punicoes.iter_mut().filter(|p| p.ja_cumpridos < p.total_a_cumprir) {
                        
                        let servicos_restantes = punido.total_a_cumprir - punido.ja_cumpridos;
                        if servicos_restantes == 1 && adiar_neste_dia {
                            continue;
                        }

                        if let Some(user_punido) = todos_utilizadores.iter().find(|u| u.id == punido.user_id) {
                            
                            let punido_e_elegivel = !exclusao_hoje.contains(&user_punido.id)
                                && !utilizadores_ja_alocados_hoje.contains(&user_punido.id)
                                && posto.turmas_permitidas.contains(&user_punido.ano)
                                && (match &posto.funcao_exclusiva { Some(f) => user_punido.roles.contains(f), None => true })
                                && (match posto.genero {
                                    Genero::Masculino => user_punido.genero == Genero::Masculino,
                                    Genero::Feminino => user_punido.genero == Genero::Feminino,
                                    Genero::Misto => true,
                                });

                            if punido_e_elegivel {
                                if let Some(candidato_substituido) = candidatos_ao_posto.iter().find(|c| !utilizadores_ja_alocados_hoje.contains(&c.id)) {
                                    id_para_contagem = Some(candidato_substituido.id.clone());
                                }
                                alocacao_final = Some(Alocacao {
                                    user_id: user_punido.id.clone(),
                                    nome: format!("{} ({}/{})", user_punido.name, punido.ja_cumpridos + 1, punido.total_a_cumprir),
                                    punicao: true,
                                });
                                punido.ja_cumpridos += 1;
                                break; 
                            }
                        }
                    }
                }

                // 2. SE NENHUM PUNIDO FOI ALOCADO, PROSSEGUE COM A LÓGICA NORMAL
                if alocacao_final.is_none() {
                    if let Some(candidato_justo) = candidatos_ao_posto.iter().find(|c| !utilizadores_ja_alocados_hoje.contains(&c.id) && match posto.genero {
                        Genero::Masculino => c.genero == Genero::Masculino,
                        Genero::Feminino => c.genero == Genero::Feminino,
                        Genero::Misto => true,
                    }) {
                        let mut pessoa_alocada: Option<User> = Some(candidato_justo.clone());
                        let mut divida_foi_paga = false;

                        if let Some(lista_dividas) = dividas.get(candidato_justo.id.as_str()) {
                            for divida in lista_dividas {
                                let divida_e_compativel = matches!((&divida.tipo_divida, &tipo_contagem), (TipoServico::RN, TipoServico::RN) | (_, TipoServico::RD));
                                if divida_e_compativel {
                                    if let Some(devedor) = todos_utilizadores.iter().find(|u| u.id == divida.credor) {
                                        let devedor_e_elegivel = !exclusao_hoje.contains(&devedor.id) && !utilizadores_ja_alocados_hoje.contains(&devedor.id) &&
                                                                 posto.turmas_permitidas.contains(&devedor.ano) &&
                                                                 (match &posto.funcao_exclusiva { Some(f) => devedor.roles.contains(f), None => true }) &&
                                                                 (match posto.genero {
                                                                     Genero::Masculino => devedor.genero == Genero::Masculino,
                                                                     Genero::Feminino => devedor.genero == Genero::Feminino,
                                                                     Genero::Misto => true,
                                                                 });
                                        if devedor_e_elegivel {
                                            pessoa_alocada = Some(devedor.clone());
                                            dividas_pagas.push((candidato_justo.id.clone(), divida.clone()));
                                            divida_foi_paga = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        
                        if let Some(alocado) = pessoa_alocada {
                             alocacao_final = Some(Alocacao {
                                user_id: alocado.id.clone(),
                                nome: if divida_foi_paga { format!("{} (PG)", alocado.name) } else { alocado.name.clone() },
                                punicao: false,
                            });
                            id_para_contagem = Some(alocado.id.clone());
                        }
                    }
                }
                
                // 3. EFETIVA A ALOCAÇÃO E CONTAGEM
                if let Some(alocacao) = alocacao_final {
                    escala_do_posto.insert(horario.clone(), alocacao.clone());
                    utilizadores_ja_alocados_hoje.push(alocacao.user_id.clone());
                    if let Some(user_id_contagem) = id_para_contagem {
                        let contagem_do_user = contagens.entry(user_id_contagem).or_default();
                        match tipo_contagem {
                            TipoServico::RN => contagem_do_user.rn += 1,
                            _ => contagem_do_user.rd += 1,
                        }
                    }
                } else {
                    return Err(format!("Não foi possível encontrar um candidato para o Posto '{}' no horário '{}' do dia {}.", posto.nome, horario, data).into());
                }
            }
            escala_do_dia.insert(posto.nome.clone(), escala_do_posto);
        }

        // --- GERAÇÃO DA EQUIPA DE RETÉM ---
        let mut equipe_retem: Vec<Alocacao> = Vec::new();
        let ids_punidos: Vec<String> = punicoes.iter().map(|p| p.user_id.clone()).collect();
        let mut exclusao_retem = exclusao_hoje.clone();
        exclusao_retem.extend(ids_punidos);

        let quotas_retem = [(3, 2), (2, 2), (1, 4)]; // (Ano, Quantidade)

        for (ano, quantidade) in quotas_retem {
            let mut candidatos_retem: Vec<&User> = todos_utilizadores
                .iter()
                .filter(|u| u.ano == ano)
                .filter(|u| !exclusao_retem.contains(&u.id))
                .filter(|u| !utilizadores_ja_alocados_hoje.contains(&u.id))
                .collect();
            
            candidatos_retem.sort_by(|a, b| {
                let cont_a = contagens.get(&a.id).map_or(0, |c| c.retem);
                let cont_b = contagens.get(&b.id).map_or(0, |c| c.retem);
                cont_a.cmp(&cont_b).then_with(|| {
                    let num_a: u32 = a.id.chars().skip(1).collect::<String>().parse().unwrap_or(0);
                    let num_b: u32 = b.id.chars().skip(1).collect::<String>().parse().unwrap_or(0);
                    num_b.cmp(&num_a)
                })
            });

            for candidato in candidatos_retem.iter().take(quantidade) {
                let alocacao = Alocacao {
                    user_id: candidato.id.clone(),
                    nome: candidato.name.clone(),
                    punicao: false,
                };
                equipe_retem.push(alocacao);
                utilizadores_ja_alocados_hoje.push(candidato.id.clone());
                contagens.entry(candidato.id.clone()).or_default().retem += 1;
            }
        }

        utilizadores_fadigados = utilizadores_ja_alocados_hoje;

        let json_diario = serde_json::to_string_pretty(&EscalaDiaria { 
            tipo_dia: tipo_dia.clone(), 
            escala: escala_do_dia,
            retem: equipe_retem,
        })?;
        let filename = format!("{}/{}.json", ESCALA_DATA_DIR, data.format("%Y-%m-%d"));
        fs::write(filename, json_diario).await?;
    }

    // Salvar o estado final dos ficheiros de contagem, dívidas e punições
    for (credor_id, divida_paga) in dividas_pagas {
        if let Some(lista_dividas) = dividas.get_mut(&credor_id) {
            if let Some(pos) = lista_dividas.iter().position(|d| d == &divida_paga) {
                lista_dividas.remove(pos);
            }
            if lista_dividas.is_empty() {
                dividas.remove(&credor_id);
            }
        }
    }
    fs::write(CONTAGEM_FILE, serde_json::to_string_pretty(&contagens)?).await?;
    fs::write(DIVIDAS_FILE, serde_json::to_string_pretty(&dividas)?).await?;
    fs::write(PUNIDOS_FILE, serde_json::to_string_pretty(&punicoes)?).await?;
    
    println!("Processo de geração de escala concluído com sucesso!");
    Ok(())
}


// --- LÓGICA DE CRIAÇÃO DE FICHEIROS ---
pub async fn ensure_escala_structure() {
    if let Err(e) = fs::create_dir_all(ESCALA_DATA_DIR).await {
        eprintln!("🔥 Falha crítica ao criar o diretório '{}': {}", ESCALA_DATA_DIR, e);
        return;
    }
    
    if fs::try_exists(ESTADO_FILE).await.unwrap_or(false) == false {
        let default_estado = EstadoEscala {
            periodo_atual: Periodo {
                start_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2025, 1, 7).unwrap(),
            },
            periodo_seguinte: None,
            status_trocas: "Fechado".to_string(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&default_estado) {
            let _ = fs::write(ESTADO_FILE, json).await;
        }
    }

    if fs::try_exists(POSTOS_FILE).await.unwrap_or(false) == false {
        let _ = fs::write(POSTOS_FILE, "[]").await;
    }
    if fs::try_exists(CONTAGEM_FILE).await.unwrap_or(false) == false {
        let _ = fs::write(CONTAGEM_FILE, "{}").await;
    }
    if fs::try_exists(DIVIDAS_FILE).await.unwrap_or(false) == false {
        let _ = fs::write(DIVIDAS_FILE, "{}").await;
    }
    if fs::try_exists(TROCAS_FILE).await.unwrap_or(false) == false {
        let _ = fs::write(TROCAS_FILE, "[]").await;
    }
    if fs::try_exists(INDISPONIBILIDADE_FILE).await.unwrap_or(false) == false {
        let _ = fs::write(INDISPONIBILIDADE_FILE, "[]").await;
    }
    if fs::try_exists(PUNIDOS_FILE).await.unwrap_or(false) == false {
        let _ = fs::write(PUNIDOS_FILE, "[]").await;
    }
    if fs::try_exists(CONFIGURACAO_FILE).await.unwrap_or(false) == false {
        let _ = fs::write(CONFIGURACAO_FILE, r#"{ "postos_punicao": [] }"#).await;
    }
}
