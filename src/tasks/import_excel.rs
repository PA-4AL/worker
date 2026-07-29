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
    /// Correspondance explicite « colonne logique → lettre Excel », ex.
    /// `{"Équipe": "A", "Pseudo": "C", "Rang": "D"}`.
    ///
    /// Sans elle, les colonnes sont retrouvées par leur **en-tête**, ce qui
    /// impose au fichier de porter exactement les libellés attendus. Or un
    /// organisateur reçoit les inscriptions dans le format de son choix : lui
    /// demander de renommer ses colonnes avant d'importer est une friction
    /// inutile quand il peut simplement désigner lesquelles utiliser.
    #[serde(default)]
    columns: Option<HashMap<String, String>>,
    /// La première ligne est-elle une ligne d'en-têtes ?
    ///
    /// Vrai par défaut. N'a de sens qu'avec `columns` : sans correspondance
    /// explicite, l'en-tête est indispensable pour identifier les colonnes.
    #[serde(default)]
    has_header: Option<bool>,
}

/// Convertit une référence de colonne Excel en indice : `A` → 0, `Z` → 25,
/// `AA` → 26. Les lettres sont acceptées en minuscules.
fn colonne_en_indice(reference: &str) -> Result<usize, WorkerError> {
    let reference = reference.trim().to_uppercase();
    if reference.is_empty() || !reference.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(WorkerError::InvalidPayload(format!(
            "Référence de colonne invalide : « {reference} » (attendu A, B, … AA)"
        )));
    }
    // Base 26 sans zéro : la position de A vaut 1, d'où le -1 final.
    let mut indice = 0usize;
    for c in reference.chars() {
        indice = indice * 26 + (c as usize - 'A' as usize + 1);
    }
    Ok(indice - 1)
}

/// Import d'équipes depuis un fichier Excel.
///
/// Chaque ligne du fichier est un joueur rattaché à une équipe via la
/// colonne commune "Équipe". Le worker regroupe les joueurs par équipe
/// (dans l'ordre d'apparition) et renvoie la liste des équipes prêtes
/// à être inscrites au tournoi par le backend.
pub async fn execute(payload: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
    let payload: ImportExcelPayload =
        serde_json::from_value(payload).map_err(|e| WorkerError::InvalidPayload(e.to_string()))?;

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

    // Toutes les lignes d'abord : avec une correspondance explicite et sans
    // en-tête, la première ligne est une ligne de données et ne doit pas être
    // consommée pour rien.
    let toutes: Vec<&[Data]> = range.rows().collect();
    let premiere = *toutes
        .first()
        .ok_or_else(|| WorkerError::ExcelError("File is empty".into()))?;

    let headers: Vec<String> = premiere
        .iter()
        .map(|cell| match cell {
            Data::String(s) => s.trim().to_string(),
            Data::Float(f) => {
                if f.fract() == 0.0 {
                    (*f as i64).to_string()
                } else {
                    f.to_string()
                }
            }
            Data::Int(i) => i.to_string(),
            _ => String::new(),
        })
        .collect();

    let mut expected = vec![TEAM_COLUMN];
    expected.extend(parser.expected_columns());

    // Avec une correspondance explicite, les libellés du fichier n'ont plus
    // d'importance : seule compte la position désignée par l'utilisateur.
    if let Some(mapping) = &payload.columns {
        let mut index = HashMap::new();
        for col in &expected {
            let reference = mapping
                .get(*col)
                .ok_or_else(|| WorkerError::MissingColumn((*col).to_string()))?;
            index.insert((*col).to_string(), colonne_en_indice(reference)?);
        }
        return construire_equipes(
            toutes
                .into_iter()
                .skip(if payload.has_header.unwrap_or(true) {
                    1
                } else {
                    0
                }),
            &*parser,
            &index,
        );
    }

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

    let saute_entete = payload.has_header.unwrap_or(true);
    construire_equipes(
        toutes.into_iter().skip(if saute_entete { 1 } else { 0 }),
        &*parser,
        &col_index,
    )
}

/// Regroupe les joueurs par équipe, dans l'ordre d'apparition du fichier.
///
/// Isolée pour être partagée par les deux façons d'identifier les colonnes —
/// par en-tête ou par correspondance explicite. Seule la construction de
/// `col_index` diffère ; tout le reste, y compris la validation ligne à ligne,
/// doit rester identique.
fn construire_equipes<'a>(
    rows: impl Iterator<Item = &'a [Data]>,
    parser: &dyn parser::traits::SchemaParser,
    col_index: &HashMap<String, usize>,
) -> Result<serde_json::Value, WorkerError> {
    let mut team_order: Vec<String> = Vec::new();
    let mut teams: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut player_count: usize = 0;

    for (i, row) in rows.enumerate() {
        if row.iter().all(|c| matches!(c, Data::Empty)) {
            continue;
        }
        let line = i + 2; // ligne Excel réelle (1 = en-têtes)

        let team_name = cell_str(row, col_index, TEAM_COLUMN)?;
        if team_name.is_empty() {
            return Err(WorkerError::ParseError(format!(
                "Colonne '{TEAM_COLUMN}' vide à la ligne {line}"
            )));
        }

        let player = parser
            .parse_row(row, col_index)
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

    #[test]
    fn conversion_des_references_de_colonne() {
        assert_eq!(colonne_en_indice("A").unwrap(), 0);
        assert_eq!(colonne_en_indice("C").unwrap(), 2);
        assert_eq!(colonne_en_indice("Z").unwrap(), 25);
        // Base 26 sans zéro : AA suit Z, ce n'est pas 0*26+0.
        assert_eq!(colonne_en_indice("AA").unwrap(), 26);
        assert_eq!(colonne_en_indice("AB").unwrap(), 27);
        // Tolérant sur la casse et les espaces : la saisie vient d'une interface.
        assert_eq!(colonne_en_indice(" b ").unwrap(), 1);
        assert!(colonne_en_indice("").is_err());
        assert!(colonne_en_indice("A1").is_err());
        assert!(colonne_en_indice("é").is_err());
    }

    #[tokio::test]
    async fn une_correspondance_explicite_ignore_les_entetes() {
        // Le fichier porte des libellés qui ne sont PAS ceux attendus, et une
        // colonne parasite au milieu : sans correspondance explicite, l'import
        // échouerait sur une colonne manquante.
        let fichier = xlsx_base64(&[
            &["Team", "Ville", "Joueur", "Niveau"],
            &["Les Renards", "Lyon", "alice", "Diamant"],
            &["Les Renards", "Lyon", "bob", "Or"],
            &["Nova", "Paris", "carol", "Platine"],
        ]);

        let resultat = execute(serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": fichier,
            "columns": { "Équipe": "A", "Pseudo": "C", "Rang": "D" },
        }))
        .await
        .expect("l'import doit réussir malgré des en-têtes inattendus");

        assert_eq!(resultat["team_count"], 2);
        assert_eq!(resultat["player_count"], 3);
        assert_eq!(resultat["teams"][0]["name"], "Les Renards");
        assert_eq!(resultat["teams"][0]["players"][0]["username"], "alice");
        assert_eq!(resultat["teams"][0]["players"][0]["rank"], "Diamant");
        assert_eq!(resultat["teams"][1]["players"][0]["username"], "carol");
    }

    #[tokio::test]
    async fn un_fichier_sans_entete_est_lisible() {
        // Certaines exports d'inscriptions n'ont pas de ligne de titre : la
        // première ligne est alors une donnée, et la perdre amputerait une équipe.
        let fichier = xlsx_base64(&[
            &["Les Renards", "alice", "Diamant"],
            &["Nova", "carol", "Platine"],
        ]);

        let resultat = execute(serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": fichier,
            "columns": { "Équipe": "A", "Pseudo": "B", "Rang": "C" },
            "has_header": false,
        }))
        .await
        .expect("l'import doit réussir sans ligne d'en-tête");

        assert_eq!(resultat["team_count"], 2);
        assert_eq!(resultat["player_count"], 2);
    }

    #[tokio::test]
    async fn une_correspondance_incomplete_est_refusee() {
        // Mieux vaut refuser que produire des joueurs sans rang en silence.
        let fichier = xlsx_base64(&[&["Les Renards", "alice", "Diamant"]]);

        let erreur = execute(serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": fichier,
            "columns": { "Équipe": "A", "Pseudo": "B" },
            "has_header": false,
        }))
        .await
        .expect_err("une colonne attendue non mappée doit échouer");

        assert!(
            matches!(erreur, WorkerError::MissingColumn(_)),
            "{erreur:?}"
        );
    }

    #[tokio::test]
    async fn une_reference_de_colonne_invalide_est_refusee() {
        let fichier = xlsx_base64(&[&["Les Renards", "alice", "Diamant"]]);

        let erreur = execute(serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": fichier,
            "columns": { "Équipe": "A", "Pseudo": "B", "Rang": "3" },
            "has_header": false,
        }))
        .await
        .expect_err("une lettre de colonne invalide doit échouer");

        assert!(
            matches!(erreur, WorkerError::InvalidPayload(_)),
            "{erreur:?}"
        );
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
        let file = xlsx_base64(&[&["Pseudo", "Rang"], &["alice", "Diamant"]]);
        let payload = serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": file,
        });

        let err = execute(payload).await.unwrap_err();
        assert!(matches!(err, WorkerError::MissingColumn(c) if c == TEAM_COLUMN));
    }

    #[tokio::test]
    async fn import_echoue_si_cellule_equipe_vide() {
        let file = xlsx_base64(&[&["Équipe", "Pseudo", "Rang"], &["", "alice", "Diamant"]]);
        let payload = serde_json::json!({
            "tournament_type": "esport_5v5",
            "file_base64": file,
        });

        let err = execute(payload).await.unwrap_err();
        assert!(err.to_string().contains("ligne 2"));
    }
}
