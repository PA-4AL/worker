use super::traits::SchemaParser;

/// Schéma Esport 5v5 : colonnes Équipe | Pseudo | Rang.
/// (La colonne Équipe est la clé de regroupement, pas un champ joueur.)
pub struct EsportParser;

impl SchemaParser for EsportParser {
    fn player_fields(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("username", "Pseudo"),
            ("rank", "Rang"),
        ]
    }
}
