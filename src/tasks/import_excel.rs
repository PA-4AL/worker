use base64::{engine::general_purpose::STANDARD, Engine};
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Cursor;

use crate::errors::WorkerError;
use crate::parser;
use crate::parser::traits::{cell_str, TEAM_COLUMN};

#[derive(Debug, Deserialize)]
struct ImportExcelPayload {
    tournament_type: String,
    file_base64: String,
}

/// Import d'équipes depuis un fichier Excel.
///
/// Chaque ligne du fichier est un joueur rattaché à une équipe via la
/// colonne commune "Équipe". Le worker regroupe les joueurs par équipe
/// (dans l'ordre d'apparition) et renvoie la liste des équipes prêtes
/// à être inscrites au tournoi par le backend.
pub async fn execute(payload: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
    let payload: ImportExcelPayload = serde_json::from_value(payload)
        .map_err(|e| WorkerError::InvalidPayload(e.to_string()))?;

    let bytes = STANDARD.decode(&payload.file_base64)?;

    let cursor = Cursor::new(bytes);
    let mut workbook =
        open_workbook_auto_from_rs(cursor).map_err(|e| WorkerError::ExcelError(e.to_string()))?;

    let sheet_names = workbook.sheet_names().to_vec();
    let sheet_name = sheet_names
        .first()
        .ok_or_else(|| WorkerError::ExcelError("No sheets found".into()))?
        .clone();

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| WorkerError::ExcelError(e.to_string()))?;

    let parser = parser::get_parser(&payload.tournament_type)?;

    let mut rows = range.rows();

    let headers: Vec<String> = rows
        .next()
        .ok_or_else(|| WorkerError::ExcelError("File is empty".into()))?
        .iter()
        .map(|cell| match cell {
            Data::String(s) => s.trim().to_string(),
            Data::Float(f) => {
                if f.fract() == 0.0 { (*f as i64).to_string() } else { f.to_string() }
            }
            Data::Int(i) => i.to_string(),
            _ => String::new(),
        })
        .collect();

    let mut expected = vec![TEAM_COLUMN];
    expected.extend(parser.expected_columns());
    for col in expected {
        if !headers.iter().any(|h| h == col) {
            return Err(WorkerError::MissingColumn(col.to_string()));
        }
    }

    let col_index: HashMap<String, usize> = headers
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    // Regroupe les joueurs par équipe, dans l'ordre d'apparition du fichier.
    let mut team_order: Vec<String> = Vec::new();
    let mut teams: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut player_count: usize = 0;

    for (i, row) in rows.enumerate() {
        if row.iter().all(|c| matches!(c, Data::Empty)) {
            continue;
        }
        let line = i + 2; // ligne Excel réelle (1 = en-têtes)

        let team_name = cell_str(row, &col_index, TEAM_COLUMN)?;
        if team_name.is_empty() {
            return Err(WorkerError::ParseError(format!(
                "Colonne '{TEAM_COLUMN}' vide à la ligne {line}"
            )));
        }

        let player = parser
            .parse_row(row, &col_index)
            .map_err(|e| WorkerError::ParseError(format!("Ligne {line}: {e}")))?;

        if !teams.contains_key(&team_name) {
            team_order.push(team_name.clone());
        }
        teams.entry(team_name).or_default().push(player);
        player_count += 1;
    }

    if team_order.is_empty() {
        return Err(WorkerError::ParseError(
            "Aucune équipe trouvée dans le fichier".into(),
        ));
    }

    let teams_json: Vec<serde_json::Value> = team_order
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "players": teams[name],
            })
        })
        .collect();

    Ok(serde_json::json!({
        "team_count": teams_json.len(),
        "player_count": player_count,
        "teams": teams_json,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_xlsxwriter::Workbook;

    /// Construit un fichier Excel en mémoire et le renvoie encodé en base64.
    fn xlsx_base64(rows: &[&[&str]]) -> String {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in row.iter().enumerate() {
                sheet.write_string(r as u32, c as u16, *value).unwrap();
            }
        }
        let bytes = workbook.save_to_buffer().unwrap();
        STANDARD.encode(bytes)
    }

    #[tokio::test]
    async fn import_regroupe_les_joueurs_par_equipe() {
        let file = xlsx_base64(&[
            &["Équipe", "Pseudo", "Rang"],
            &["Les Renards", "alice", "Diamant"],
            &["Les Renards", "bob", "Or"],
            &["Nova", "carol", "Platine"],
        ]);
        let payload = serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": file,
        });

        let result = execute(payload).await.unwrap();

        assert_eq!(result["team_count"], 2);
        assert_eq!(result["player_count"], 3);
        assert_eq!(result["teams"][0]["name"], "Les Renards");
        assert_eq!(result["teams"][0]["players"][0]["username"], "alice");
        assert_eq!(result["teams"][1]["name"], "Nova");
        assert_eq!(result["teams"][1]["players"][0]["rank"], "Platine");
    }

    #[tokio::test]
    async fn import_echoue_si_colonne_equipe_absente() {
        let file = xlsx_base64(&[
            &["Pseudo", "Rang"],
            &["alice", "Diamant"],
        ]);
        let payload = serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": file,
        });

        let err = execute(payload).await.unwrap_err();
        assert!(matches!(err, WorkerError::MissingColumn(c) if c == TEAM_COLUMN));
    }

    #[tokio::test]
    async fn import_echoue_si_cellule_equipe_vide() {
        let file = xlsx_base64(&[
            &["Équipe", "Pseudo", "Rang"],
            &["", "alice", "Diamant"],
        ]);
        let payload = serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": file,
        });

        let err = execute(payload).await.unwrap_err();
        assert!(err.to_string().contains("ligne 2"));
    }
}
