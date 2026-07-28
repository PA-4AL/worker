pub mod esport;
pub mod football;
pub mod traits;

use crate::errors::WorkerError;
use esport::EsportParser;
use football::FootballParser;
use traits::SchemaParser;

/// Retourne le parser correspondant au type de tournoi.
/// Ajouter un nouveau type de tournoi = ajouter un parser ici.
pub fn get_parser(tournament_type: &str) -> Result<Box<dyn SchemaParser>, WorkerError> {
    match tournament_type {
        "football_11v11" => Ok(Box::new(FootballParser)),
        "esport_5v5" => Ok(Box::new(EsportParser)),
        other => Err(WorkerError::ParseError(format!(
            "Unknown tournament type: {other}"
        ))),
    }
}
