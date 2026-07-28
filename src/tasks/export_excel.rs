use base64::{engine::general_purpose::STANDARD, Engine};
use rust_xlsxwriter::{Format, Workbook, Worksheet};
use serde::Deserialize;
use std::collections::HashMap;

use crate::errors::WorkerError;
use crate::parser;
use crate::parser::traits::TEAM_COLUMN;

#[derive(Debug, Deserialize)]
struct ExportExcelPayload {
    tournament_type: String,
    tournament_name: String,
    #[serde(default)]
    teams: Vec<Team>,
    #[serde(default)]
    matches: Vec<Match>,
}

#[derive(Debug, Deserialize)]
struct Team {
    name: String,
    #[serde(default)]
    players: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Match {
    round: u32,
    team_a: String,
    team_b: String,
    #[serde(default)]
    score_a: Option<i64>,
    #[serde(default)]
    score_b: Option<i64>,
    /// "pending" | "in_progress" | "finished"
    #[serde(default)]
    status: String,
}

impl Match {
    fn is_finished(&self) -> bool {
        self.status == "finished" && self.score_a.is_some() && self.score_b.is_some()
    }

    fn winner(&self) -> Option<&str> {
        if !self.is_finished() {
            return None;
        }
        match (self.score_a, self.score_b) {
            (Some(a), Some(b)) if a > b => Some(&self.team_a),
            (Some(a), Some(b)) if b > a => Some(&self.team_b),
            _ => None, // égalité
        }
    }
}

#[derive(Default)]
struct Standing {
    played: i64,
    wins: i64,
    draws: i64,
    losses: i64,
    diff: i64,
    points: i64,
}

/// Export de l'état d'un tournoi vers un fichier Excel.
///
/// Le worker n'a pas d'accès à la base : le backend envoie l'état complet
/// (équipes + matchs) dans le payload, à n'importe quel moment du tournoi.
/// Le fichier généré contient trois feuilles : Équipes, Matchs, Classement.
pub async fn execute(payload: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
    let payload: ExportExcelPayload =
        serde_json::from_value(payload).map_err(|e| WorkerError::InvalidPayload(e.to_string()))?;

    let parser = parser::get_parser(&payload.tournament_type)?;

    let mut workbook = Workbook::new();
    let header = Format::new().set_bold();

    write_teams_sheet(
        workbook.add_worksheet(),
        &header,
        &payload.teams,
        parser.player_fields(),
    )?;
    write_matches_sheet(workbook.add_worksheet(), &header, &payload.matches)?;
    write_standings_sheet(workbook.add_worksheet(), &header, &payload)?;

    let bytes = workbook
        .save_to_buffer()
        .map_err(|e| WorkerError::ExcelWriteError(e.to_string()))?;

    Ok(serde_json::json!({
        "file_name": file_name(&payload.tournament_name),
        "file_base64": STANDARD.encode(bytes),
    }))
}

/// Feuille 1 — Équipes : une ligne par joueur, mêmes colonnes que l'import.
fn write_teams_sheet(
    sheet: &mut Worksheet,
    header: &Format,
    teams: &[Team],
    player_fields: &[(&str, &str)],
) -> Result<(), WorkerError> {
    sheet
        .set_name("Équipes")
        .map_err(|e| WorkerError::ExcelWriteError(e.to_string()))?;

    let mut write = SheetWriter::new(sheet);
    let mut columns = vec![TEAM_COLUMN];
    columns.extend(player_fields.iter().map(|(_, col)| *col));
    write.header_row(header, &columns)?;

    for team in teams {
        if team.players.is_empty() {
            write.row(&[team.name.as_str()])?;
            continue;
        }
        for player in &team.players {
            let mut cells = vec![team.name.clone()];
            for (key, _) in player_fields {
                cells.push(json_cell(player, key));
            }
            write.row_owned(&cells)?;
        }
    }
    Ok(())
}

/// Feuille 2 — Matchs : tous les matchs, joués ou non.
fn write_matches_sheet(
    sheet: &mut Worksheet,
    header: &Format,
    matches: &[Match],
) -> Result<(), WorkerError> {
    sheet
        .set_name("Matchs")
        .map_err(|e| WorkerError::ExcelWriteError(e.to_string()))?;

    let mut write = SheetWriter::new(sheet);
    write.header_row(
        header,
        &[
            "Round",
            "Équipe A",
            "Score A",
            "Score B",
            "Équipe B",
            "Statut",
            "Vainqueur",
        ],
    )?;

    for m in matches {
        let score = |s: Option<i64>| s.map(|v| v.to_string()).unwrap_or_default();
        write.row_owned(&[
            m.round.to_string(),
            m.team_a.clone(),
            score(m.score_a),
            score(m.score_b),
            m.team_b.clone(),
            status_label(&m.status).to_string(),
            m.winner().unwrap_or_default().to_string(),
        ])?;
    }
    Ok(())
}

/// Feuille 3 — Classement calculé à partir des matchs terminés
/// (victoire 3 pts, nul 1 pt), trié par points puis différence de score.
fn write_standings_sheet(
    sheet: &mut Worksheet,
    header: &Format,
    payload: &ExportExcelPayload,
) -> Result<(), WorkerError> {
    sheet
        .set_name("Classement")
        .map_err(|e| WorkerError::ExcelWriteError(e.to_string()))?;

    let mut standings: HashMap<&str, Standing> = payload
        .teams
        .iter()
        .map(|t| (t.name.as_str(), Standing::default()))
        .collect();

    for m in payload.matches.iter().filter(|m| m.is_finished()) {
        let (a, b) = (m.score_a.unwrap_or(0), m.score_b.unwrap_or(0));
        for (team, my_score, other_score) in [(&m.team_a, a, b), (&m.team_b, b, a)] {
            let entry = standings.entry(team.as_str()).or_default();
            entry.played += 1;
            entry.diff += my_score - other_score;
            if my_score > other_score {
                entry.wins += 1;
                entry.points += 3;
            } else if my_score < other_score {
                entry.losses += 1;
            } else {
                entry.draws += 1;
                entry.points += 1;
            }
        }
    }

    let mut ranked: Vec<(&str, Standing)> = standings.into_iter().collect();
    ranked.sort_by(|(name_a, s_a), (name_b, s_b)| {
        (s_b.points, s_b.diff)
            .cmp(&(s_a.points, s_a.diff))
            .then_with(|| name_a.cmp(name_b))
    });

    let mut write = SheetWriter::new(sheet);
    write.header_row(
        header,
        &[
            "Position",
            TEAM_COLUMN,
            "Joués",
            "Victoires",
            "Nuls",
            "Défaites",
            "Diff",
            "Points",
        ],
    )?;

    for (position, (name, s)) in ranked.iter().enumerate() {
        write.row_owned(&[
            (position + 1).to_string(),
            (*name).to_string(),
            s.played.to_string(),
            s.wins.to_string(),
            s.draws.to_string(),
            s.losses.to_string(),
            s.diff.to_string(),
            s.points.to_string(),
        ])?;
    }
    Ok(())
}

/// Petit assistant d'écriture ligne par ligne dans une feuille.
struct SheetWriter<'a> {
    sheet: &'a mut Worksheet,
    row: u32,
}

impl<'a> SheetWriter<'a> {
    fn new(sheet: &'a mut Worksheet) -> Self {
        Self { sheet, row: 0 }
    }

    fn header_row(&mut self, format: &Format, cells: &[&str]) -> Result<(), WorkerError> {
        for (c, value) in cells.iter().enumerate() {
            self.sheet
                .write_string_with_format(self.row, c as u16, *value, format)
                .map_err(|e| WorkerError::ExcelWriteError(e.to_string()))?;
            self.sheet
                .set_column_width(c as u16, (value.chars().count().max(12)) as f64 + 2.0)
                .map_err(|e| WorkerError::ExcelWriteError(e.to_string()))?;
        }
        self.row += 1;
        Ok(())
    }

    fn row(&mut self, cells: &[&str]) -> Result<(), WorkerError> {
        for (c, value) in cells.iter().enumerate() {
            self.sheet
                .write_string(self.row, c as u16, *value)
                .map_err(|e| WorkerError::ExcelWriteError(e.to_string()))?;
        }
        self.row += 1;
        Ok(())
    }

    fn row_owned(&mut self, cells: &[String]) -> Result<(), WorkerError> {
        let refs: Vec<&str> = cells.iter().map(String::as_str).collect();
        self.row(&refs)
    }
}

/// Valeur d'un champ joueur en texte (les joueurs sont des objets JSON libres).
fn json_cell(player: &serde_json::Value, key: &str) -> String {
    match player.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn status_label(status: &str) -> &str {
    match status {
        "pending" => "À venir",
        "in_progress" => "En cours",
        "finished" => "Terminé",
        other => other,
    }
}

/// "Coupe d'été 2026" → "export_coupe_d_ete_2026.xlsx" (sans accents ni espaces).
fn file_name(tournament_name: &str) -> String {
    let slug: String = tournament_name
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    let slug = slug.trim_matches('_').to_string();
    let slug = if slug.is_empty() {
        "tournoi".to_string()
    } else {
        slug
    };
    format!("export_{slug}.xlsx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{open_workbook_auto_from_rs, Data, Reader};
    use std::io::Cursor;

    fn payload_demo() -> serde_json::Value {
        serde_json::json!({
            "tournament_type": "esport_5v5",
            "tournament_name": "Coupe d'été",
            "teams": [
                { "name": "Nova", "players": [
                    { "username": "carol", "rank": "Platine" }
                ]},
                { "name": "Les Renards", "players": [
                    { "username": "alice", "rank": "Diamant" },
                    { "username": "bob", "rank": "Or" }
                ]},
            ],
            "matches": [
                { "round": 1, "team_a": "Les Renards", "team_b": "Nova",
                  "score_a": 2, "score_b": 1, "status": "finished" },
                { "round": 2, "team_a": "Nova", "team_b": "Les Renards",
                  "status": "pending" },
            ],
        })
    }

    #[tokio::test]
    async fn export_genere_les_trois_feuilles() {
        let result = execute(payload_demo()).await.unwrap();

        assert_eq!(result["file_name"], "export_coupe_d_ete.xlsx");

        let bytes = STANDARD
            .decode(result["file_base64"].as_str().unwrap())
            .unwrap();
        let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes)).unwrap();
        assert_eq!(
            workbook.sheet_names(),
            vec!["Équipes", "Matchs", "Classement"]
        );

        // Feuille Équipes : 1 en-tête + 3 joueurs
        let teams = workbook.worksheet_range("Équipes").unwrap();
        assert_eq!(teams.rows().count(), 4);

        // Classement : Les Renards (3 pts) devant Nova (0 pt)
        let standings = workbook.worksheet_range("Classement").unwrap();
        let rows: Vec<Vec<Data>> = standings.rows().map(|r| r.to_vec()).collect();
        assert_eq!(rows[1][1], Data::String("Les Renards".into()));
        assert_eq!(rows[1][7], Data::String("3".into()));
        assert_eq!(rows[2][1], Data::String("Nova".into()));
    }

    #[tokio::test]
    async fn export_match_non_termine_sans_vainqueur() {
        let result = execute(payload_demo()).await.unwrap();
        let bytes = STANDARD
            .decode(result["file_base64"].as_str().unwrap())
            .unwrap();
        let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes)).unwrap();

        let matches = workbook.worksheet_range("Matchs").unwrap();
        let rows: Vec<Vec<Data>> = matches.rows().map(|r| r.to_vec()).collect();
        // Match 1 terminé : vainqueur affiché
        assert_eq!(rows[1][6], Data::String("Les Renards".into()));
        // Match 2 à venir : statut traduit, pas de vainqueur
        assert_eq!(rows[2][5], Data::String("À venir".into()));
    }

    #[tokio::test]
    async fn export_refuse_un_type_de_tournoi_inconnu() {
        let payload = serde_json::json!({
            "tournament_type": "curling_2v2",
            "tournament_name": "x",
            "teams": [],
            "matches": [],
        });
        assert!(execute(payload).await.is_err());
    }
}
