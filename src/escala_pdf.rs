use std::collections::{BTreeMap, HashMap};
use chrono::NaiveDate;
use chrono::Datelike;
use genpdf::{
    SimplePageDecorator, Alignment, Document, Margins, Element
};
use genpdf::fonts::{FontFamily, FontData};
use genpdf::elements::{
    PageBreak, Paragraph, Break, LinearLayout, TableLayout, FrameCellDecorator, PaddedElement
};
use genpdf::style::Style;

use crate::auth::User;
use crate::escala::TipoServico;
use crate::escala::{EscalaDiaria, Periodo};

pub struct PdfData<'a> {
    pub periodo: &'a Periodo,
    pub escalas: &'a BTreeMap<NaiveDate, EscalaDiaria>,
    pub users: &'a HashMap<String, User>,
    pub info_assinatura_fixa: (&'a str, &'a str),
    pub info_assinatura_dinamica: (&'a str, &'a str),
}

// Função auxiliar para formatar horários
fn formatar_horario(horario: &str) -> String {
    horario
        .split('/')
        .map(|periodo| {
            periodo
                .split('-')
                .map(|hora| {
                    let hora_limpa = hora.replace(":", "");
                    match hora_limpa.len() {
                        3 => format!("0{}", hora_limpa),
                        4 => hora_limpa,
                        1 => format!("000{}", hora_limpa),
                        2 => format!("00{}", hora_limpa),
                        _ => hora_limpa,
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn weekday_pt_br(date: &NaiveDate) -> &'static str {
    match date.weekday() {
        chrono::Weekday::Mon => "SEGUNDA-FEIRA",
        chrono::Weekday::Tue => "TERÇA-FEIRA",
        chrono::Weekday::Wed => "QUARTA-FEIRA",
        chrono::Weekday::Thu => "QUINTA-FEIRA",
        chrono::Weekday::Fri => "SEXTA-FEIRA",
        chrono::Weekday::Sat => "SÁBADO",
        chrono::Weekday::Sun => "DOMINGO",
    }
}

fn tipo_rotina_str(tipo: &TipoServico) -> &'static str {
    match tipo {
        TipoServico::RN => "ROTINA NORMAL",
        _ => "ROTINA DE DOMINGO",
    }
}

fn criar_estilos() -> (Style, Style, Style) {
    (
        Style::new().with_font_size(8),
        Style::new().bold().with_font_size(9),
        Style::new().bold().with_font_size(11),
    )
}

fn cabecalho_instituicao() -> Vec<impl Element> {
    vec![
        Paragraph::new("CENTRO DE INSTRUÇÃO ALMIRANTE GRAÇA ARANHA").aligned(Alignment::Center).styled(Style::new().bold().with_font_size(10)),
        Paragraph::new("ESCOLA DE FORMAÇÃO DE OFICIAIS DA MARINHA MERCANTE").aligned(Alignment::Center).styled(Style::new().bold().with_font_size(10)),
        Paragraph::new("DETALHE DE SERVIÇO DO CORPO DE ALUNOS DA EFOMM").aligned(Alignment::Center).styled(Style::new().bold().with_font_size(10)),
    ]
}

fn bloco_assinatura(fixa: (&str, &str), dinamica: (&str, &str)) -> PaddedElement<TableLayout> {
    let mut sig_tbl = TableLayout::new(vec![1, 1]);
    sig_tbl.set_cell_decorator(FrameCellDecorator::new(false, false, false));
    let left = {
        let mut l = LinearLayout::vertical();
        l.push(Paragraph::new("__________________________").aligned(Alignment::Center));
        l.push(Paragraph::new(fixa.0).aligned(Alignment::Center));
        l.push(Paragraph::new(fixa.1).aligned(Alignment::Center).styled(Style::new().italic()));
        l
    };
    let right = {
        let mut l = LinearLayout::vertical();
        l.push(Paragraph::new("__________________________").aligned(Alignment::Center));
        l.push(Paragraph::new(dinamica.0).aligned(Alignment::Center));
        l.push(Paragraph::new(dinamica.1).aligned(Alignment::Center).styled(Style::new().italic()));
        l
    };
    let mut row = sig_tbl.row();
    row.push_element(left);
    row.push_element(right);
    row.push().expect("signature row");
    PaddedElement::new(sig_tbl, Margins::trbl(20, 0, 0, 0))
}

fn tabela_diario(postos_diario: &[&str], escala_diaria: &EscalaDiaria, users: &HashMap<String, User>, default_style: &Style, header_style: &Style) -> TableLayout {
    let mut table = TableLayout::new(vec![1, 2, 1, 2]);
    let mut i = 0;
    while i < postos_diario.len() {
        let mut row = table.row();
        for j in 0..2 {
            if i + j < postos_diario.len() {
                let posto = postos_diario[i + j];
                let aloc = &escala_diaria.escala[posto]["DIARIO"];
                let txt = users
                    .get(&aloc.user_id)
                    .map(|u| format!("{}{} {}", u.curso, u.id, aloc.nome))
                    .unwrap_or_else(|| aloc.nome.clone());
                row.push_element(Paragraph::new(posto).styled(header_style.clone()));
                row.push_element(Paragraph::new(txt).styled(default_style.clone().italic()));
            } else {
                row.push_element(Paragraph::new(""));
                row.push_element(Paragraph::new(""));
            }
        }
        row.push().expect("row push");
        i += 2;
    }
    table
}

fn tabela_turnos(postos_turnos: &[&str], horarios: &[String], escala_diaria: &EscalaDiaria, users: &HashMap<String, User>, default_style: &Style, header_style: &Style) -> TableLayout {
    let mut widths = vec![8]; // coluna do posto
    let col_count = if horarios.is_empty() { 1 } else { horarios.len() };
    widths.extend(std::iter::repeat(8).take(col_count)); // aumenta largura das colunas de horários
    let mut table = TableLayout::new(widths);
    // Header
    {
        let mut row = table.row();
        row.push_element(Paragraph::new("").styled(header_style.clone()));
        if horarios.is_empty() {
            row.push_element(Paragraph::new("Horário").styled(header_style.clone()));
        } else {
            for h in horarios {
                let pretty = formatar_horario(h);
                if pretty.contains('\n') {
                    let mut layout = LinearLayout::vertical();
                    for linha in pretty.split('\n') {
                        layout.push(Paragraph::new(linha).styled(header_style.clone()));
                    }
                    row.push_element(layout);
                } else {
                    row.push_element(Paragraph::new(pretty).styled(header_style.clone()));
                }
            }
        }
        row.push().expect("header row");
    }
    // Data rows
    for posto in postos_turnos {
        let mut row = table.row();
        let posto_trim = posto.trim();
        // Nome do posto em negrito
        row.push_element(Paragraph::new(posto_trim).styled(header_style.clone()));
        let map = escala_diaria.escala.get(posto_trim);
        if horarios.is_empty() {
            row.push_element(Paragraph::new("---").styled(default_style.clone()));
        } else {
            for h in horarios {
                let txt = map
                    .and_then(|m| m.get(h))
                    .map(|aloc| {
                        users
                            .get(&aloc.user_id)
                            .map(|u| format!("{}{} {}", u.curso, u.id, aloc.nome))
                            .unwrap_or_else(|| aloc.nome.clone())
                    })
                    .unwrap_or_else(|| "---".to_string());
                // Nome do aluno em itálico
                row.push_element(Paragraph::new(txt).styled(default_style.clone().italic()));
            }
        }
        row.push().expect("turno row");
    }
    table
}

fn tabela_retem(escala_diaria: &EscalaDiaria, users: &HashMap<String, User>, default_style: &Style, header_style: &Style) -> Option<TableLayout> {
    if escala_diaria.retem.is_empty() {
        return None;
    }
    let mut table = TableLayout::new(vec![1, 5]);
    let mut por_ano: BTreeMap<u8, Vec<String>> = BTreeMap::new();
    for aloc in &escala_diaria.retem {
        if let Some(u) = users.get(&aloc.user_id) {
            por_ano.entry(u.ano).or_default().push(format!("{}{} {}",u.curso, u.id, aloc.nome));
        }
    }
    for (ano, nomes) in por_ano.iter().rev() {
        let mut row = table.row();
        row.push_element(Paragraph::new(format!("{}º ANO", ano)).styled(header_style.clone()));
        row.push_element(Paragraph::new(nomes.join(", ")).styled(default_style.clone()));
        row.push().expect("retem row");
    }
    Some(table)
}

pub fn gerar_pdf_da_escala_ativa(data: PdfData) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // 1. Load font
    let regular_data = include_bytes!("../fonts/LiberationSans-Regular.ttf").to_vec();
    let bold_data = include_bytes!("../fonts/LiberationSans-Bold.ttf").to_vec();
    let italic_data = include_bytes!("../fonts/LiberationSans-Italic.ttf").to_vec();
    let bold_italic_data = include_bytes!("../fonts/LiberationSans-BoldItalic.ttf").to_vec();
    let font_family = FontFamily {
        regular: FontData::new(regular_data, None)?,
        bold: FontData::new(bold_data, None)?,
        italic: FontData::new(italic_data, None)?,
        bold_italic: FontData::new(bold_italic_data, None)?,
    };
    // 2. Create document
    let mut doc = Document::new(font_family);
    doc.set_title("Escala de Serviço");
    let mut decorator = SimplePageDecorator::new();
    decorator.set_margins(10);
    doc.set_page_decorator(decorator);
    // 3. Build pages
    let mut first_day = true;
    for (date, escala_diaria) in data.escalas {
        let mut page_content = LinearLayout::vertical();
        for cab in cabecalho_instituicao() { page_content.push(cab); page_content.push(Break::new(0.1)); }
        let title = Paragraph::new(format!(
            "{} - {}, {}",
            tipo_rotina_str(&escala_diaria.tipo_dia),
            weekday_pt_br(date),
            date.format("%d/%m/%Y")
        )).aligned(Alignment::Center)
        .styled(Style::new().bold().with_font_size(9));
        page_content.push(title);
        page_content.push(Break::new(2.0));
        let (default_style, header_style, section_title_style) = criar_estilos();
        let seccoes = vec![
            ("3º ANO", vec!["AJOSCA", "RANCHEIRO", "CHEFE DE DIA"]),
            ("2º ANO", vec!["SALÃO DE VÍDEO", "SALÃO DE RECREIO", "LOJA SAMM", "SUBCHEFE", "CONFERÊNCIA", "GARAGEM", "POLÍCIA", "COPA",]),
            ("1º ANO", vec!["ENTREGADOR", "PAV 3A", "PAV 3B", "PAV 2", "GUARDA PAV FEM", "RONDA", "CLAVICULÁRIO", "PAV 2 - FEM", "LAVANDERIA"]),
            //("PAV FEM", vec!["PAV 2 - FEM", "LAVANDERIA"]),
        ];
        let mut any_section = false;
        for (titulo_seccao, nomes_postos) in seccoes {
            let mut postos_diario = Vec::new();
            let mut postos_turnos = Vec::new();
            let mut horarios_set  = BTreeMap::new();
            for &posto in &nomes_postos {
                if let Some(map) = escala_diaria.escala.get(posto) {
                    if map.contains_key("DIARIO") {
                        postos_diario.push(posto);
                    } else if !map.is_empty() {
                        postos_turnos.push(posto);
                        for k in map.keys() {
                            horarios_set.insert(k.clone(), ());
                        }
                    } else {
                        // Posto existe mas não tem alocação, mostrar mesmo assim
                        postos_turnos.push(posto);
                    }
                } else {
                    // Posto não existe no mapa, mostrar mesmo assim
                    postos_turnos.push(posto);
                }
            }
            if postos_diario.is_empty() && postos_turnos.is_empty() {
                continue;
            }
            any_section = true;
            // Título da seção alinhado à esquerda
            page_content.push(Paragraph::new(titulo_seccao).aligned(Alignment::Left).styled(section_title_style.clone()));
            page_content.push(Break::new(1.0));
            if !postos_diario.is_empty() {
                // Serviços diários mais próximos: menos espaçamento após a tabela
                let table = tabela_diario(&postos_diario, escala_diaria, data.users, &default_style, &header_style);
                page_content.push(table);
                page_content.push(Break::new(0.2));
            }
            if !postos_turnos.is_empty() {
                let mut horarios: Vec<String> = horarios_set.keys().cloned().collect();
                if horarios.is_empty() {
                    horarios.push("".to_string());
                }
                // Espaçamento maior entre horários e nomes
                let table = tabela_turnos(&postos_turnos, &horarios, escala_diaria, data.users, &default_style, &header_style);
                page_content.push(Break::new(0.2));
                page_content.push(table);
                page_content.push(Break::new(1.0));
            }
            // Remove a linha horizontal acima dos anos (não adicionar mais)
        }
        if let Some(table) = tabela_retem(escala_diaria, data.users, &default_style, &header_style) {
            any_section = true;
            // Título do retém alinhado à esquerda
            page_content.push(Paragraph::new("RETÉM").aligned(Alignment::Left).styled(section_title_style.clone()));
            page_content.push(Break::new(0.5));
            page_content.push(table);
            page_content.push(Break::new(0.5));
            // Caixa com texto centralizado abaixo do retém
            use chrono::Local;
            let data_hoje = Local::now().format("%d/%m/%Y").to_string();
            let texto = format!("Documento gerado em {}", data_hoje);
            let mut caixa = LinearLayout::vertical();
            caixa.push(Paragraph::new("").styled(default_style.clone()));
            caixa.push(Paragraph::new(&texto).aligned(Alignment::Center).styled(default_style.clone()));
            caixa.push(Paragraph::new("").styled(default_style.clone()));
            let boxed = PaddedElement::new(caixa, Margins::trbl(4, 4, 4, 4));
            page_content.push(boxed);
            page_content.push(Break::new(0.5));
        }
        page_content.push(bloco_assinatura(data.info_assinatura_fixa, data.info_assinatura_dinamica));
        if any_section {
            if !first_day {
                doc.push(PageBreak::new());
            }
            first_day = false;
            doc.push(page_content);
        }
    }
    // 4. Render
    let mut buf = Vec::new();
    doc.render(&mut buf)?;
    Ok(buf)
}