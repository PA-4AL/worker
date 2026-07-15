# PA Tournament — Worker (Rust)

Worker asynchrone de la plateforme de gestion de tournois esport.
Il tourne en boucle, consomme les demandes du backend via **Google Cloud
Pub/Sub** (`topic-demandes`), les traite, puis publie le résultat dans
`topic-réponses`. Il est complètement isolé du backend : **aucun accès à la
base de données**, aucune communication synchrone.

Documentation : [`DOC.md`](DOC.md) (architecture, format des messages) ·
[`DOC_TECHNIQUE.md`](DOC_TECHNIQUE.md) (explication fichier par fichier) ·
[`PROJET.md`](PROJET.md) (suivi) · specs : [`docs/PA-Tournament-Specs.md`](docs/PA-Tournament-Specs.md).

## Tâches

| `task_type` | Rôle | Retry |
|---|---|---|
| `import_excel` | Fichier Excel (base64) → équipes + rosters formatés, regroupés par la colonne `Équipe`, prêts à inscrire au tournoi | 3 tentatives, backoff exponentiel |
| `export_excel` | État du tournoi (équipes, matchs, scores) → fichier `.xlsx` (feuilles Équipes / Matchs / Classement), à tout moment du tournoi | 1 tentative |

Chaque type de tournoi a son schéma Excel (trait `SchemaParser`) :
`football_11v11` (Équipe, Nom, Prénom, Poste, Numéro) et `esport_5v5`
(Équipe, Pseudo, Rang). Ajouter un type = un fichier dans `src/parser/`.

## Démarrer

```bash
cp .env.example .env   # renseigner le projet GCP et les queues Pub/Sub
cargo run
```

## Tests

```bash
cargo test             # fabrique de vrais fichiers Excel en mémoire
```
