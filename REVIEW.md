# Compte-rendu — Review du code + Alternatives techniques

**Date :** 2026-06-08
**Version reviewée :** worker v0.1.0
**Statut global :** Fonctionnel, architecture solide — 2 bugs à corriger, 4 améliorations importantes

---

## Table des matières

1. [Bugs identifiés (à corriger)](#1-bugs-identifiés-à-corriger)
2. [Problèmes de qualité (non bloquants)](#2-problèmes-de-qualité-non-bloquants)
3. [Alternatives aux méthodes utilisées](#3-alternatives-aux-méthodes-utilisées)
4. [Ce qui est bien fait](#4-ce-qui-est-bien-fait)
5. [Résumé priorisé](#5-résumé-priorisé)

---

## 1. Bugs identifiés (à corriger)

### Bug #1 — Panic possible dans `execute_with_retry` si `max_attempts = 0`

**Fichier :** `src/retry/mod.rs` — ligne 65

**Le code problématique :**
```rust
Err((last_error.unwrap(), policy.max_attempts))
//              ^^^^^^^^ PANIC si last_error est None
```

**Pourquoi ça peut planter :** Si quelqu'un configure `max_attempts: 0`, la boucle `for attempt in 0..0` ne s'exécute jamais. `last_error` reste `None`. Appeler `.unwrap()` sur `None` provoque un **panic** (crash du programme).

**Correction :**
```rust
// Remplacer :
Err((last_error.unwrap(), policy.max_attempts))

// Par :
match last_error {
    Some(e) => Err((e, policy.max_attempts)),
    None    => Err((WorkerError::ParseError("max_attempts est 0".into()), 0)),
}
```

**Criticité : Moyenne** — N'arrive pas avec la config actuelle (min 1 tentative), mais fragile si on change les paramètres.

---

### Bug #2 — Dépendances inutilisées dans `Cargo.toml`

**Fichier :** `Cargo.toml` — lignes 22 et 24

```toml
bytes = "1"   # ← plus utilisé nulle part dans le code
uuid = { version = "1", features = ["v4"] }  # ← jamais utilisé
```

`bytes` était utilisé dans `producer.rs` avant la correction (on utilisait `Bytes::from(...)`). Il a été supprimé du code mais pas du `Cargo.toml`. `uuid` avait été prévu pour générer les `task_id`, mais le worker ne génère pas les IDs — c'est le backend qui le fait.

**Conséquence :** Ces deux crates sont compilées et téléchargées sans servir à rien. `cargo check` ne le signale pas, mais `cargo +nightly udeps` le détecte.

**Correction :**
```toml
# Supprimer ces deux lignes :
bytes = "1"
uuid = { version = "1", features = ["v4"] }
```

**Criticité : Faible** — Pas de bug fonctionnel, mais alourdit la compilation et crée de la confusion.

---

## 2. Problèmes de qualité (non bloquants)

### Problème #3 — Parsing Excel bloque le runtime Tokio *(Important)*

**Fichier :** `src/tasks/import_excel.rs` — ligne 16

**Ce qui se passe :**
```rust
pub async fn execute(payload: serde_json::Value) -> Result<...> {
    // ...
    let bytes = STANDARD.decode(&payload.file_base64)?;  // CPU
    let mut workbook = open_workbook_auto_from_rs(cursor)?; // CPU (lecture mémoire)
    // ... toute la boucle de parsing est CPU
    // Il n'y a AUCUN .await dans cette fonction
}
```

La fonction est déclarée `async` mais n'a aucune opération réellement asynchrone. Elle fait uniquement du travail **CPU intensif** (décodage base64, parsing Excel, boucle sur les lignes).

**Le problème concret :** Tokio est un runtime asynchrone qui tourne sur un nombre limité de threads (par défaut : 1 thread par cœur CPU). Si une tâche async bloque un thread pendant 2 secondes (un gros fichier Excel), **toutes les autres tâches async sur ce thread sont gelées** pendant ce temps — dont le pull de nouveaux messages, le ping vers Pub/Sub, etc.

**La bonne pratique Rust/Tokio :**
```rust
pub async fn execute(payload: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
    // Déléguer le travail CPU à un thread dédié (thread pool de blocking)
    tokio::task::spawn_blocking(move || {
        execute_sync(payload)  // fonction synchrone identique
    })
    .await
    .map_err(|e| WorkerError::ParseError(e.to_string()))?
}

fn execute_sync(payload: serde_json::Value) -> Result<serde_json::Value, WorkerError> {
    // ... même code qu'avant, mais fn normale sans async
}
```

`spawn_blocking` envoie le travail sur un thread pool séparé (par défaut 512 threads) réservé aux opérations bloquantes. Les threads Tokio restent libres.

**Criticité : Haute** — Devient un vrai problème dès qu'un fichier Excel est grand (> 1000 lignes) ou que le worker reçoit plusieurs messages en rafale.

---

### Problème #4 — `gcp_project_id` stocké mais jamais utilisé

**Fichier :** `src/config.rs` — ligne 5

```rust
pub struct Config {
    pub gcp_project_id: String, // ← warning "dead code" à la compilation
    // ...
}
```

Le projet GCP est déjà inclus dans les chemins complets `subscription_demands` et `topic_responses` (ex: `projects/MON_PROJET/subscriptions/...`). Le champ `gcp_project_id` est donc redondant.

**Deux options :**

Option A — Le supprimer simplement :
```rust
pub struct Config {
    pub subscription_demands: String,
    pub topic_responses: String,
}
```

Option B — L'utiliser pour construire les chemins automatiquement (plus propre) :
```rust
pub struct Config {
    pub gcp_project_id: String,
    pub subscription_name: String,  // juste "sub-demandes" sans le préfixe
    pub topic_name: String,         // juste "topic-reponses" sans le préfixe
}

impl Config {
    pub fn subscription_path(&self) -> String {
        format!("projects/{}/subscriptions/{}", self.gcp_project_id, self.subscription_name)
    }
    pub fn topic_path(&self) -> String {
        format!("projects/{}/topics/{}", self.gcp_project_id, self.topic_name)
    }
}
```

**Criticité : Faible** — Seulement un warning de compilation. Pas de bug.

---

### Problème #5 — Messages traités séquentiellement

**Fichier :** `src/main.rs` — ligne 80

```rust
Ok(mut messages) => {
    for msg in &mut messages {  // ← traitement un par un
        // ...
    }
}
```

Le worker pull jusqu'à 10 messages en une fois, mais les traite **l'un après l'autre**. Si chaque message prend 1 seconde (parsing d'un gros fichier), 10 messages prennent 10 secondes.

**Alternative avec traitement concurrent :**
```rust
use futures::stream::{self, StreamExt};

Ok(messages) => {
    stream::iter(messages)
        .for_each_concurrent(4, |mut msg| async move { // 4 messages en parallèle
            // ... même logique
        })
        .await;
}
```

Nécessite d'ajouter `futures = "0.3"` dans `Cargo.toml`.

**Criticité : Faible pour l'instant** — Devient importante si le volume de messages est élevé.

---

### Problème #6 — Aucune limite de taille sur le payload

**Fichier :** `src/tasks/import_excel.rs` — ligne 20

```rust
let bytes = STANDARD.decode(&payload.file_base64)?;
```

Si quelqu'un envoie un fichier Excel de 500 Mo encodé en base64 (667 Mo de texte), le worker va tenter de tout charger en mémoire et peut crasher par OOM (Out Of Memory).

**Correction simple :**
```rust
const MAX_FILE_SIZE_MB: usize = 50;
const MAX_BASE64_LEN: usize = MAX_FILE_SIZE_MB * 1024 * 1024 * 4 / 3;

if payload.file_base64.len() > MAX_BASE64_LEN {
    return Err(WorkerError::InvalidPayload(
        format!("Fichier trop volumineux (max {}MB)", MAX_FILE_SIZE_MB)
    ));
}
let bytes = STANDARD.decode(&payload.file_base64)?;
```

**Criticité : Moyenne** — Risque de sécurité si le backend ne valide pas lui-même la taille du fichier avant de l'envoyer.

---

### Problème #7 — Message mal formé : aucune réponse envoyée au backend

**Fichier :** `src/main.rs` — lignes 103-106

```rust
Err(e) => {
    tracing::error!(error = %e, "Cannot parse incoming message, skipping");
    // ← le message est acké mais aucune réponse n'est publiée
}
```

Si le JSON entrant est malformé (champ manquant, mauvais type), le message est silencieusement abandonné. Le backend attend une réponse qui n'arrivera jamais.

**Le problème** : sans `task_id`, on ne peut pas corréler la réponse. C'est un cas limite difficile à gérer proprement.

**Option recommandée** : logger l'erreur avec les données brutes (pour débug) et ne pas bloquer — l'ACK actuel est correct. Mais documenter que le backend doit avoir un **timeout** sur ses demandes.

---

## 3. Alternatives aux méthodes utilisées

---

### Alt. A — Retry : `backoff` crate au lieu du code custom

**Méthode actuelle :** Boucle `for` manuelle avec `tokio::time::sleep` et calcul exponentiel maison (`initial_delay * 2^attempt`).

**Alternative : crate `backoff`**

```toml
# Cargo.toml
backoff = { version = "0.4", features = ["tokio"] }
```

```rust
use backoff::{ExponentialBackoff, future::retry, Error};

let result = retry(ExponentialBackoff::default(), || async {
    import_excel::execute(payload.clone()).await
        .map_err(Error::transient) // toutes les erreurs → réessayer
}).await;
```

**Avantages :**
- Gère aussi le **jitter** (légère randomisation du délai pour éviter que tous les workers retry en même temps)
- Configurable finement (délai max, multiplicateur, max elapsed time)
- Bibliothèque battle-tested

**Inconvénients :**
- Dépendance externe supplémentaire
- Moins de contrôle sur la politique par type de tâche
- `backoff` ne distingue pas les erreurs "réessayables" des erreurs définitives sans code supplémentaire

**Verdict : Pour ce projet, le code custom est suffisant et plus lisible.**

---

### Alt. B — Configuration : `envy` crate au lieu du code manuel

**Méthode actuelle :** Appels manuels à `std::env::var()` un par un, avec conversion d'erreur à la main.

**Alternative : crate `envy`**

```toml
envy = "0.4"
```

```rust
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub gcp_project_id: String,
    pub pubsub_subscription_demands: String,  // noms en snake_case = noms des env vars
    pub pubsub_topic_responses: String,
}

impl Config {
    pub fn from_env() -> Result<Self, envy::Error> {
        envy::from_env()  // une seule ligne !
    }
}
```

`envy` utilise serde pour désérialiser les variables d'environnement directement dans la struct. Les noms de champs en snake_case correspondent automatiquement aux noms de variables en majuscules (`PUBSUB_SUBSCRIPTION_DEMANDS` → `pubsub_subscription_demands`).

**Avantages :**
- Code minimal (1 ligne vs 10)
- Toutes les variables manquantes sont reportées en une seule erreur
- Cohérent avec l'approche serde du reste du code

**Inconvénients :**
- Dépendance supplémentaire
- Messages d'erreur légèrement moins personnalisables

**Verdict : Meilleure approche pour ce projet — moins de code, plus robuste.**

---

### Alt. C — Parser : enum dispatch au lieu de `Box<dyn Trait>`

**Méthode actuelle :** Trait objet dynamique — `Box<dyn SchemaParser>`. Le compilateur ne connaît pas le type exact à la compilation. L'appel de méthode passe par une **vtable** (table de pointeurs de fonctions) à l'exécution.

**Alternative : enum dispatch**

```rust
pub enum Parser {
    Football(FootballParser),
    Esport(EsportParser),
}

impl Parser {
    pub fn from_type(tournament_type: &str) -> Result<Self, WorkerError> {
        match tournament_type {
            "football_11v11" => Ok(Parser::Football(FootballParser)),
            "esport_5v5"     => Ok(Parser::Esport(EsportParser)),
            other            => Err(WorkerError::ParseError(...)),
        }
    }

    pub fn expected_columns(&self) -> &'static [&'static str] {
        match self {
            Parser::Football(p) => p.expected_columns(),
            Parser::Esport(p)   => p.expected_columns(),
        }
    }

    pub fn parse_row(&self, ...) -> Result<Value, WorkerError> {
        match self {
            Parser::Football(p) => p.parse_row(...),
            Parser::Esport(p)   => p.parse_row(...),
        }
    }
}
```

**Avantages :**
- **Dispatch statique** = légèrement plus rapide (pas de vtable, compilateur peut inliner)
- Pas d'allocation heap (`Box`)
- Le compilateur force à gérer tous les variants → plus sûr

**Inconvénients :**
- Ajouter un type de tournoi nécessite de modifier l'enum **ET** tous les `match` — risque d'oubli
- Avec `Box<dyn Trait>`, ajouter un parser = juste créer un fichier et une ligne dans `get_parser`

**Verdict : `Box<dyn Trait>` est le meilleur choix ici car le projet va s'étendre (ajout fréquent de types de tournois).**

---

### Alt. D — Traitement des messages : streaming vs polling

**Méthode actuelle :** Pull périodique (`consumer.pull(10)` en boucle avec `sleep(500ms)` si vide).

**Alternative : `Subscription::subscribe()`**

La crate `google-cloud-pubsub` offre aussi un mode streaming :

```rust
let mut stream = subscription.subscribe(None).await?;
while let Some(mut msg) = stream.recv().await {
    let data = msg.message.data.clone();
    // traiter...
    msg.ack().await?;
}
```

**Avantages :**
- Pas besoin du `sleep(500ms)` — le stream bloque naturellement jusqu'au prochain message
- Latence légèrement plus faible (pas d'attente artificielle entre messages)
- Code plus simple

**Inconvénients :**
- L'API `subscribe` gère moins bien le backpressure (traitement concurrent limité)
- Moins de contrôle sur le nombre de messages récupérés à la fois
- Le mode Pull est plus explicite et débogable

**Verdict : Pour ce projet, le mode Pull est plus clair et suffisant. Le streaming serait à considérer pour un volume très élevé (> 1000 msg/s).**

---

### Alt. E — Concurrence : traitement parallèle des messages

**Méthode actuelle :** Traitement séquentiel (1 message à la fois même si 10 sont pullés).

**Alternative : `FuturesUnordered` ou `for_each_concurrent`**

```rust
use futures::stream::{self, StreamExt};

// Traiter jusqu'à 4 messages en parallèle
stream::iter(messages)
    .for_each_concurrent(Some(4), |mut msg| async move {
        let data = msg.message.data.clone();
        if let Ok(incoming) = serde_json::from_slice::<IncomingMessage>(&data) {
            let response = dispatcher::dispatch(&incoming).await;
            let _ = producer.publish(&response).await;
        }
        let _ = msg.ack().await;
    })
    .await;
```

**Avantages :**
- Meilleure utilisation des ressources
- Latence divisée par le facteur de concurrence

**Inconvénients :**
- Ordre d'exécution non garanti (OK pour Pub/Sub qui ne garantit pas l'ordre de toute façon)
- Nécessite que `producer` soit partageable (il faut `Arc<QueueProducer>`)
- Plus complexe à déboguer

**Verdict : À implémenter quand le volume de messages augmente. Pas nécessaire maintenant.**

---

### Alt. F — Gestion d'erreur : `anyhow` partout vs `thiserror`

**Méthode actuelle :** `thiserror` pour créer un `enum WorkerError` typé avec des variants spécifiques.

**Alternative : `anyhow` partout**

```rust
// Avec anyhow, n'importe quelle erreur est acceptable :
pub async fn execute(payload: Value) -> anyhow::Result<Value> {
    let bytes = STANDARD.decode(&file_base64)
        .context("Échec du décodage base64")?;
    // ...
}
```

**Avantages de `anyhow` :**
- Moins de boilerplate
- Messages d'erreur avec contexte empilé (`context(...)`)
- Parfait pour du code applicatif "leaf" (les extrémités)

**Inconvénients de `anyhow` :**
- Perd le type d'erreur → impossible de faire `match` sur `WorkerError::MissingColumn`
- Impossible de mapper proprement vers `error_code()` dans `dispatcher.rs`

**La bonne pratique Rust :** utiliser `thiserror` dans les bibliothèques et le code "métier" (où le type d'erreur est important), et `anyhow` dans `main.rs` (niveau applicatif où on veut juste afficher ou propager).

**Verdict : Le choix actuel (`thiserror` pour le domaine métier, `anyhow` dans `main`) est le plus idiomatique. Bien joué.**

---

## 4. Ce qui est bien fait

| Point | Explication |
|---|---|
| Séparation des responsabilités | Chaque module a un rôle unique et bien délimité |
| Graceful shutdown complet | SIGINT + SIGTERM gérés, shutdown token propagé |
| ACK toujours exécuté | Même en cas d'erreur de parsing, le message est acké — pas de boucle infinie |
| Réponse d'erreur toujours envoyée | `dispatch()` retourne toujours un `WorkerResponse`, jamais d'erreur non gérée |
| `Box<dyn SchemaParser>` | Bon choix pour extensibilité (ajout futur de types de tournois) |
| `thiserror` pour le domaine | `error_code()` permet de mapper proprement les erreurs vers le JSON |
| `anyhow` dans `main` | Pattern idiomatique Rust : `anyhow` pour le niveau application |
| `skip_serializing_if` sur `data`/`error` | JSON propre sans champs nuls inutiles |
| Logs structurés avec `tracing` | `task_id` et `task_type` dans chaque log — facilite le débogage en prod |
| Isolation totale du worker | Aucun accès BDD, aucun appel HTTP vers le backend — architecture respectée |

---

## 5. Résumé priorisé

### À corriger maintenant

| # | Fichier | Problème | Action |
|---|---|---|---|
| 1 | `retry/mod.rs:65` | Panic si `max_attempts = 0` | Remplacer `.unwrap()` par un `match` |
| 2 | `Cargo.toml:22,24` | `bytes` et `uuid` inutilisés | Supprimer les deux lignes |

### À améliorer (avant la mise en production)

| # | Fichier | Problème | Action |
|---|---|---|---|
| 3 | `tasks/import_excel.rs` | Parsing CPU bloque Tokio | Envelopper dans `spawn_blocking` |
| 4 | `tasks/import_excel.rs` | Aucune limite de taille | Ajouter une vérification `len > MAX` |
| 5 | `config.rs` | `gcp_project_id` inutilisé | Supprimer ou l'utiliser pour construire les paths |

### À faire pour compléter le projet

| # | Quoi | Priorité |
|---|---|---|
| 6 | Écrire des tests unitaires pour les parsers | Haute |
| 7 | Remplacer config manuelle par `envy` | Moyenne |
| 8 | Ajouter la tâche 2 (mail / PDF) | Selon le cahier des charges |
| 9 | Traitement concurrent des messages | Faible (volume actuel) |

---

*Review effectuée le 2026-06-08 — 15 fichiers analysés, 2 bugs, 5 améliorations, 6 alternatives évaluées*
