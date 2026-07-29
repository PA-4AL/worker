# Décisions d'architecture — worker

Format, règles et modèle : [`infra/docs/adr/README.md`](https://github.com/PA-4AL/infra/blob/main/docs/adr/README.md).

| N° | Décision | Date | Statut |
|---|---|---|---|
| [0001](0001-rust-et-tokio.md) | Rust et Tokio pour le worker | 2026-01-29 | accepté |
| [0002](0002-aucun-acces-a-la-base.md) | Aucun accès à la base de données | 2026-07-15 | accepté |
| [0003](0003-fichiers-en-base64-dans-le-message.md) | Transporter les fichiers en base64 dans le message | 2026-07-15 | accepté |
| [0004](0004-retry-par-criticite-de-tache.md) | Une politique de retry par criticité de tâche | 2026-07-15 | accepté |
| [0005](0005-sonde-http-sans-framework.md) | Sonde HTTP minimale, sans framework web | 2026-07-28 | accepté |
| [0006](0006-consommation-en-pull.md) | Consommer la file en pull | 2026-07-28 | accepté |
