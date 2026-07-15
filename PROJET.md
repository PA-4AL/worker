# Suivi du projet — Worker (Rust)

## Contexte
Site web de **gestion et création de tournois** comprenant :
- Arbres de tournois (brackets)
- Gestion des équipes
- Gestion des joueurs
- Import de données depuis Excel

## Objectif du worker
Traiter les opérations asynchrones/lourdes du site, en commençant par l'**import de joueurs depuis un fichier Excel** vers la base de données.

---

## État général

| Statut | Signification |
|--------|---------------|
| [ ]    | À faire       |
| [~]    | En cours      |
| [x]    | Terminé       |

---

## Phases

### Phase 1 — Initialisation du projet
- [ ] Créer le projet Rust (`cargo new`)
- [ ] Définir la structure des dossiers
- [ ] Configurer `Cargo.toml` (dépendances de base)

### Phase 2 — Architecture du worker
- [ ] Définir le rôle du worker (tâches traitées, format des messages)
- [ ] Choisir le mécanisme de communication (file de messages, canal, HTTP, etc.)
- [ ] Modéliser les structures de données principales

### Phase 3 — Implémentation
- [ ] Boucle principale du worker
- [ ] Gestion des tâches / messages entrants
- [ ] Gestion des erreurs et retry
- [ ] Logging

### Phase 4 — Tests
- [ ] Tests unitaires
- [ ] Tests d'intégration

### Phase 5 — Déploiement
- [ ] Dockerfile / configuration de déploiement
- [ ] Documentation finale

---

## Décisions techniques

| Date       | Décision | Raison |
|------------|----------|--------|
| 2026-03-26 | Langage : Rust | Performance, sécurité mémoire |
| 2026-03-26 | Frontend : React | — |
| 2026-03-26 | Backend API : Next.js (API Routes) | Fullstack React |
| 2026-03-26 | Base de données : PostgreSQL | — |
| 2026-03-26 | Queue : Google Cloud Pub/Sub | Managé, retry natif, gratuit jusqu'à 10 Go/mois, 2 topics (demandes + réponses) |
| 2026-03-26 | Schémas Excel multiples | Chaque type de tournoi a sa propre structure |

---

## Notes
- Projet démarré le 2026-03-26
