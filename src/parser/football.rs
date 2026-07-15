use super::traits::SchemaParser;

/// Schéma Football 11v11 : colonnes Équipe | Nom | Prénom | Poste | Numéro.
pub struct FootballParser;

impl SchemaParser for FootballParser {
    fn player_fields(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("last_name", "Nom"),
            ("first_name", "Prénom"),
            ("position", "Poste"),
            ("number", "Numéro"),
        ]
    }
}
