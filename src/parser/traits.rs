use std::collections::HashMap;
use calamine::Data;
use crate::errors::WorkerError;

/// Colonne commune à tous les schémas d'import : rattache chaque ligne (joueur)
/// à son équipe. C'est la clé de regroupement de l'import.
pub const TEAM_COLUMN: &str = "Équipe";

/// Récupère la valeur d'une cellule par nom de colonne et la convertit en String.
pub fn cell_str(
    row: &[Data],
    col_index: &HashMap<String, usize>,
    col: &str,
) -> Result<String, WorkerError> {
    let idx = col_index
        .get(col)
        .ok_or_else(|| WorkerError::MissingColumn(col.to_string()))?;

    let cell = row.get(*idx).unwrap_or(&Data::Empty);

    let value = match cell {
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 {
                (*f as i64).to_string()
            } else {
                f.to_string()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::Empty => String::new(),
        other => other.to_string(),
    };

    Ok(value)
}

pub trait SchemaParser: Send + Sync {
    /// Correspondance (clé JSON, colonne Excel) des champs d'un joueur.
    /// Sert à l'import (validation + parse des lignes) et à l'export
    /// (en-têtes de la feuille "Équipes").
    fn player_fields(&self) -> &'static [(&'static str, &'static str)];

    /// Colonnes attendues dans le fichier Excel (hors colonne Équipe).
    fn expected_columns(&self) -> Vec<&'static str> {
        self.player_fields().iter().map(|(_, col)| *col).collect()
    }

    /// Transforme une ligne Excel en objet joueur JSON.
    fn parse_row(
        &self,
        row: &[Data],
        col_index: &HashMap<String, usize>,
    ) -> Result<serde_json::Value, WorkerError> {
        let mut player = serde_json::Map::new();
        for (key, col) in self.player_fields() {
            let value = cell_str(row, col_index, col)?;
            player.insert((*key).to_string(), serde_json::Value::String(value));
        }
        Ok(serde_json::Value::Object(player))
    }
}
