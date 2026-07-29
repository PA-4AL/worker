# ADR-0002 — Aucun accès à la base de données

- **Date** : 2026-07-15
- **Statut** : accepté
- **Portée** : worker

## Contexte

Le brief impose que le worker soit **complètement isolé** du backend : ni accès à
sa base, ni accès à ses modèles. La spécification initiale prévoyait pourtant que
le worker polle la table `jobs` (voir l'ADR-0002 du repo `infra`).

## Décision

Le worker n'a **aucune dépendance base de données** — c'est vérifiable dans son
`Cargo.toml` : ni `sqlx`, ni `diesel`, ni pilote PostgreSQL. Il reçoit tout ce
dont il a besoin dans le message d'entrée et répond par la file de sortie, y
compris en cas d'échec.

Le backend reste **seul responsable** de la base.

## Conséquences

- isolation conforme au brief, et vérifiable en une commande
- le worker est déployable et testable sans base : ses tests tournent en mémoire
- le message doit être **autoportant**, ce qui contraint sa taille (voir ADR-0003)
- le worker ignore l'existence de la table `jobs` : c'est le backend qui traduit
  ses réponses en changements d'état
