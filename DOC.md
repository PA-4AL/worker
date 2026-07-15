# Documentation — Projet : Plateforme de gestion de tournois (BracketHub)

## Contexte du projet

Site web de **gestion et création de tournois**. Le système permet de :
- Créer et gérer des tournois (arbres de tournois, brackets)
- Gérer des équipes et des joueurs
- Importer des données (joueurs, équipes) depuis des fichiers Excel

### Stack technique

| Couche | Technologie |
|--------|-------------|
| Frontend | React |
| Backend (API) | Next.js (API Routes) |
| Base de données | PostgreSQL |
| Worker | Rust |
| Queue | Google Cloud Pub/Sub |

### Architecture globale

```
[React Frontend]
      ↓ HTTP
[Backend Next.js]
      ↓ Queue "demandes" (Backend → Worker)
[Worker Rust]
      ↓ Queue "réponses" (Worker → Backend)
[Backend Next.js]  →  [PostgreSQL]
```

> Le worker ne communique **jamais** de façon synchrone avec le backend.
> Le worker n'a **aucun accès** à la base de données.
> Il reçoit les données brutes via la queue, les **traite et formate**, puis renvoie les données propres au backend via la queue de réponses.
> C'est **le backend seul** qui est responsable de l'insertion en base de données.

---

## Exigences du projet (cahier des charges)

- [ ] Au moins **une tâche spécifique au domaine métier** (ex: validation/analyse d'un fichier Excel uploadé, génération de PDF)
- [ ] **Deux queues** : une pour les demandes (Backend → Worker), une pour les réponses (Worker → Backend)
- [ ] Le worker tourne **en boucle** en permanence
- [ ] Le worker est **complètement isolé** du backend
- [ ] Le worker gère la **politique de retry** selon la criticité de chaque tâche
- [ ] Chaque tâche en échec produit quand même **une réponse** dans la queue de réponses

---

## Tâches du worker

### Tâche 1 — `import_excel` : import d'équipes depuis Excel *(domaine métier)*
- Reçoit : fichier Excel encodé (base64), type de tournoi
- Traitement : parse le fichier selon le schéma du type de tournoi ; chaque ligne
  est un joueur rattaché à une équipe via la colonne commune **`Équipe`** ;
  le worker regroupe les joueurs par équipe (ordre d'apparition dans le fichier)
- Répond : liste des **équipes** (avec leur roster) formatées et prêtes à inscrire
  au tournoi, ou rapport d'erreurs (colonne manquante, cellule équipe vide…)
- Le backend reçoit la réponse et effectue l'insertion en base de données
- Criticité : **moyenne** — retry 3 fois avec backoff exponentiel

### Tâche 2 — `export_excel` : export de l'état du tournoi vers Excel
- Reçoit : l'état complet du tournoi (nom, type, équipes, matchs avec scores),
  envoyé par le backend **à n'importe quel moment** du tournoi
  (le worker n'a pas d'accès à la base)
- Traitement : génère un fichier `.xlsx` à trois feuilles :
  **Équipes** (rosters, mêmes colonnes que l'import), **Matchs** (rounds, scores,
  statut, vainqueur) et **Classement** (calculé sur les matchs terminés :
  victoire 3 pts, nul 1 pt, tri par points puis différence de score)
- Répond : le fichier encodé en base64 + un nom de fichier propre
- Criticité : **basse** — génération locale et déterministe, 1 tentative, pas de retry

---

## Schémas Excel multiples

Chaque type de tournoi a une **structure Excel différente**. Le worker identifie le schéma via le type de tournoi reçu dans le message.

Exemple :

La colonne **`Équipe`** est commune à tous les schémas : c'est la clé de
regroupement des joueurs. Les autres colonnes décrivent le joueur.

| Type de tournoi | Colonnes Excel attendues |
|-----------------|--------------------------|
| Football 11v11  | Équipe, Nom, Prénom, Poste, Numéro |
| Esport 5v5      | Équipe, Pseudo, Rang |

> Les schémas sont à définir au fur et à mesure des types de tournois supportés.

---

## Architecture des dossiers du worker

```
worker/
├── Cargo.toml
├── Cargo.lock
├── .env
└── src/
    ├── main.rs              # Point d'entrée, boucle principale
    ├── config.rs            # Chargement de la config (.env, variables)
    ├── queue/
    │   ├── mod.rs
    │   ├── consumer.rs      # Écoute la queue "demandes"
    │   └── producer.rs      # Publie dans la queue "réponses"
    ├── tasks/
    │   ├── mod.rs
    │   ├── dispatcher.rs    # Route le message vers la bonne tâche
    │   ├── import_excel.rs  # Tâche : parse l'Excel → équipes + rosters formatés
    │   ├── export_excel.rs  # Tâche : état du tournoi → fichier Excel (base64)
    │   └── ...              # Une tâche par fichier
    ├── parser/
    │   ├── mod.rs
    │   ├── traits.rs        # Trait SchemaParser (interface commune)
    │   ├── football.rs      # Schéma Excel Football
    │   ├── esport.rs        # Schéma Excel Esport
    │   └── ...              # Un fichier par type de tournoi
    ├── retry/
    │   └── mod.rs           # Politique de retry par tâche
    └── errors.rs            # Types d'erreurs centralisés
```

### Rôle de chaque dossier

| Dossier | Rôle |
|---------|------|
| `queue/` | Connexion, lecture et écriture dans les queues |
| `tasks/` | Logique métier : traitement et formatage des données, **pas d'accès BDD** |
| `parser/` | Parse l'Excel selon le schéma du type de tournoi |
| `retry/` | Politique de retry (nombre de tentatives, backoff) |
| `config.rs` | Lit les variables d'environnement |
| `errors.rs` | Gestion centralisée des erreurs |

---

# Documentation — Qu'est-ce qu'un Worker ?

## Définition

Un **worker** est un processus autonome dont le rôle est d'**exécuter des tâches en arrière-plan**, de manière asynchrone, sans bloquer le flux principal de l'application.

Il tourne en boucle, attend des messages dans une queue, les traite, puis publie une réponse.

---

## Fonctionnement général

```
[Backend]  →  [Queue demandes]  →  [Worker]  →  [Queue réponses]  →  [Backend]
```

1. Le backend soumet un message dans la **queue de demandes**
2. Le worker consomme le message en boucle
3. Il exécute la logique métier
4. Il publie le résultat (succès ou échec) dans la **queue de réponses**
5. Le backend lit la réponse et met à jour la base de données

---

## Caractéristiques d'un worker

| Propriété        | Description |
|------------------|-------------|
| **Autonomie**    | Tourne indépendamment, sans appel synchrone au backend |
| **Isolation**    | Pas d'accès direct à la BDD du backend |
| **Idempotence**  | Rejouer un message produit le même résultat |
| **Résilience**   | Gère les erreurs, retries, et panics sans planter |
| **Observabilité**| Expose des logs et un état de santé |

---

## Politique de retry

Pour chaque tâche, définir :
- **Criticité** : haute / moyenne / basse
- **Nombre de tentatives** : ex. 3, 5, infini
- **Stratégie** : immédiat, backoff fixe, backoff exponentiel
- **Échec final** : dead-letter queue ou réponse d'erreur dans la queue de réponses

Exemple :

| Tâche | Criticité | Retry | Stratégie |
|-------|-----------|-------|-----------|
| Import Excel | Moyenne | 3 | Backoff exponentiel |
| Envoi mail | Haute | 5 | Backoff exponentiel |
| Génération PDF | Basse | 1 | Aucun retry |

---

## En Rust — Boucle principale

```rust
#[tokio::main]
async fn main() {
    loop {
        let message = queue.receive().await;
        match dispatch(message).await {
            Ok(result) => queue_response.send(result).await,
            Err(e) => queue_response.send(ErrorResponse::from(e)).await,
        }
    }
}
```

---

## Cycle de vie

```
Démarrage → Connexion queue → Boucle infinie → Arrêt propre (graceful shutdown)
```

- **Démarrage** : connexion à la queue, chargement de la config
- **Boucle** : `loop { receive → dispatch → respond }`
- **Arrêt propre** : écoute `SIGTERM`, finit la tâche en cours avant de s'arrêter

---

## Ressources documentation

### Rust général
- **The Rust Book** (officiel) — https://doc.rust-lang.org/book/
- **Rust by Example** — https://doc.rust-lang.org/rust-by-example/

### Async / Tokio
- **Tokio** (runtime async) — https://tokio.rs/tokio/tutorial
- **Async Book** — https://rust-lang.github.io/async-book/

### Queue / Messaging — Google Cloud Pub/Sub

**Fonctionnement**

```
[Publisher]  →  [Topic]  →  [Subscription]  →  [Subscriber]
  (Backend)                                      (Worker)
```

- **Topic** : canal de communication
- **Publisher** : envoie des messages dans un topic
- **Subscription** : abonnement à un topic
- **Subscriber** : consomme les messages via une subscription

**2 topics pour ce projet**

```
topic-demandes   → Backend publie, Worker consomme (mode Pull)
topic-réponses   → Worker publie, Backend consomme (mode Pull)
```

**Mode Pull** (celui utilisé par le worker)
- Le worker interroge Pub/Sub en boucle
- Après traitement, il envoie un **ack** (acknowledgement)
- Si pas d'ack dans le délai → Pub/Sub relivrera le message automatiquement *(mécanisme de retry natif)*

**Garanties**

| Propriété | Valeur |
|-----------|--------|
| Livraison | Au moins une fois (at-least-once) |
| Ordre | Non garanti par défaut |
| Rétention | 7 jours par défaut |
| Retry automatique | Oui, si pas d'ack dans le délai |

**Avantages**
- Managé par Google → pas de serveur à maintenir
- Retry natif via le mécanisme d'ack
- Isolation totale entre backend et worker
- Gratuit jusqu'à 10 Go/mois de messages

**Crate Rust**
- **`google-cloud-pubsub`** — https://docs.rs/google-cloud-pubsub/latest/google_cloud_pubsub/

---

### Format des messages

Tous les messages sont encodés en **JSON** dans le corps du message Pub/Sub.

#### Queue `topic-demandes` — Backend → Worker

**Import d'équipes :**
```json
{
  "task_id": "uuid-v4",
  "task_type": "import_excel",
  "payload": {
    "tournament_type": "football_11v11",
    "file_base64": "<contenu du fichier Excel encodé en base64>"
  }
}
```

**Export de l'état du tournoi :**
```json
{
  "task_id": "uuid-v4",
  "task_type": "export_excel",
  "payload": {
    "tournament_type": "esport_5v5",
    "tournament_name": "Coupe d'été",
    "teams": [
      { "name": "Les Renards", "players": [ { "username": "alice", "rank": "Diamant" } ] }
    ],
    "matches": [
      { "round": 1, "team_a": "Les Renards", "team_b": "Nova",
        "score_a": 2, "score_b": 1, "status": "finished" },
      { "round": 2, "team_a": "Nova", "team_b": "Les Renards", "status": "pending" }
    ]
  }
}
```

`status` ∈ `pending` | `in_progress` | `finished`. Les scores sont optionnels
tant que le match n'est pas terminé.

| Champ | Type | Description |
|-------|------|-------------|
| `task_id` | UUID | Identifiant unique de la tâche (permet de corréler la réponse) |
| `task_type` | string | Type de tâche à exécuter (`import_excel`, ...) |
| `payload` | object | Données spécifiques à la tâche |

#### Queue `topic-réponses` — Worker → Backend

**Succès (import) :**
```json
{
  "task_id": "uuid-v4",
  "task_type": "import_excel",
  "status": "success",
  "data": {
    "team_count": 2,
    "player_count": 22,
    "teams": [
      {
        "name": "Les Aigles",
        "players": [
          { "first_name": "Alice", "last_name": "Dupont", "position": "Gardien", "number": "1" }
        ]
      }
    ]
  }
}
```

**Succès (export) :**
```json
{
  "task_id": "uuid-v4",
  "task_type": "export_excel",
  "status": "success",
  "data": {
    "file_name": "export_coupe_d_ete.xlsx",
    "file_base64": "<fichier .xlsx encodé en base64>"
  }
}
```

**Échec :**
```json
{
  "task_id": "uuid-v4",
  "task_type": "import_excel",
  "status": "error",
  "error": {
    "code": "INVALID_SCHEMA",
    "message": "Colonne 'Poste' manquante à la ligne 3",
    "attempts": 3
  }
}
```

| Champ | Type | Description |
|-------|------|-------------|
| `task_id` | UUID | Même ID que la demande (corrélation) |
| `task_type` | string | Type de tâche (pour que le backend sache quoi faire de `data`) |
| `status` | `"success"` \| `"error"` | Résultat du traitement |
| `data` | object | Données formatées, prêtes à insérer en BDD (si succès) |
| `error` | object | Détail de l'erreur et nombre de tentatives effectuées (si échec) |

> Le `task_id` est la clé de corrélation : le backend l'utilise pour savoir à quelle requête correspond la réponse.

### Lecture Excel
- **calamine** (lecture `.xlsx`) — https://docs.rs/calamine/latest/calamine/

### Gestion des erreurs
- **thiserror** — https://docs.rs/thiserror/latest/thiserror/
- **anyhow** — https://docs.rs/anyhow/latest/anyhow/
