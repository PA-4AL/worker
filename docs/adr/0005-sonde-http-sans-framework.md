# ADR-0005 — Sonde HTTP minimale, sans framework web

- **Date** : 2026-07-28
- **Statut** : accepté
- **Portée** : worker

## Contexte

Cloud Run marque en échec toute révision de type *service* qui n'écoute pas sur
le port fourni par `$PORT`, même quand le travail réel est la consommation d'une
file. Le worker n'a par ailleurs aucune raison d'exposer une API.

## Décision

Ouvrir un **écouteur TCP minimal** (`src/health.rs`, ~40 lignes avec Tokio) qui
répond `200 ok` à toute requête. Aucun framework web ajouté.

L'écouteur démarre **avant** la connexion à Pub/Sub, pour que le conteneur
réponde même si l'authentification GCP est lente.

Écartés : **axum** ou **hyper** — quelques centaines de kilooctets et un temps de
compilation supplémentaires pour une seule réponse constante ; **Cloud Run worker
pools** (type de ressource sans exigence HTTP) — plus élégant, mais support
Terraform encore incertain au moment du choix.

## Conséquences

- aucune dépendance ajoutée, binaire inchangé à quelques kilooctets près
- la révision démarre de façon fiable, et un échec d'authentification Pub/Sub
  reste lisible dans les logs au lieu de se traduire par un obscur « failed to
  listen on PORT »
- un test vérifie la réponse **et** l'arrêt propre sur annulation
- si le worker migrait vers un *worker pool*, cette sonde deviendrait inutile
