# Documentation technique — Worker Rust (BracketHub)

> **À qui s'adresse ce document ?**
> À un étudiant qui connaît les bases de la programmation mais découvre Rust et les systèmes distribués.
> Chaque terme technique est défini simplement la première fois qu'il apparaît.

---

## Table des matières

1. [C'est quoi un Worker ? Pourquoi ?](#1-cest-quoi-un-worker--pourquoi-)
2. [La méthode utilisée — Architecture générale](#2-la-méthode-utilisée--architecture-générale)
3. [Les fondamentaux Rust utilisés ici](#3-les-fondamentaux-rust-utilisés-ici)
4. [Carte de tous les fichiers](#4-carte-de-tous-les-fichiers)
5. [Fichier par fichier — explication complète](#5-fichier-par-fichier--explication-complète)
6. [Pourquoi `struct`, `enum`, `trait` ? Le choix expliqué](#6-pourquoi-struct-enum-trait--le-choix-expliqué)
7. [Le flux complet d'un message de A à Z](#7-le-flux-complet-dun-message-de-a-à-z)
8. [Les dépendances (bibliothèques externes)](#8-les-dépendances-bibliothèques-externes)

---

## 1. C'est quoi un Worker ? Pourquoi ?

### Le problème sans worker

Imagine que tu uploades un fichier Excel de 5000 joueurs sur un site web.
Le serveur doit lire le fichier, valider chaque ligne, reformater les données, puis les insérer en base de données.
Ça peut prendre 10 secondes. Pendant ce temps, le navigateur de l'utilisateur attend, bloqué.

**C'est le problème des tâches lourdes en synchrone.**

### La solution : le worker asynchrone

Au lieu de tout faire en direct, le serveur dit : "J'ai reçu ta demande, je te réponds dès que c'est traité."
Il dépose la tâche dans une **file d'attente** (queue), et un **worker** — un programme séparé qui tourne en permanence — récupère la tâche, la traite, puis renvoie le résultat.

```
Utilisateur → Backend → [Queue demandes] → Worker → [Queue réponses] → Backend → Base de données
```

> **Queue** (file d'attente) : comme une file à la caisse du supermarché. Les messages s'accumulent dans l'ordre, et le worker les traite un par un.

### Ce que fait notre worker

- Il tourne **en boucle infinie** (24h/24, 7j/7)
- Il lit des messages depuis la queue **"topic-demandes"** (envoyés par le backend Next.js)
- Il exécute la tâche (exemple : parser un fichier Excel)
- Il publie le résultat dans la queue **"topic-réponses"**
- Il ne touche **jamais** à la base de données directement — c'est le backend qui s'en charge

---

## 2. La méthode utilisée — Architecture générale

### Principe de séparation des responsabilités

Chaque dossier a **une seule responsabilité** bien définie. C'est le principe **Single Responsibility** en génie logiciel.

```
src/
├── main.rs          → Orchestration : démarre tout, boucle principale
├── config.rs        → Lire la configuration (variables d'environnement)
├── errors.rs        → Définir tous les types d'erreurs possibles
├── models.rs        → Définir la forme des messages JSON
├── queue/           → Parler à Google Pub/Sub (recevoir / envoyer)
├── tasks/           → La logique métier (ce que fait vraiment le worker)
├── parser/          → Lire et valider les fichiers Excel
└── retry/           → Réessayer une tâche qui échoue
```

### Pattern utilisé : Pipeline

Les données passent d'étape en étape comme sur un tapis roulant :

```
Message JSON brut
      ↓ (désérialisation)
IncomingMessage (structure Rust)
      ↓ (dispatch)
Tâche exécutée (ex: import_excel)
      ↓ (résultat)
WorkerResponse (structure Rust)
      ↓ (sérialisation)
Message JSON publié dans la queue réponses
```

> **Désérialisation** : transformer du texte JSON en un objet utilisable dans le code.
> **Sérialisation** : l'inverse — transformer un objet en texte JSON.

### Programmation asynchrone avec Tokio

Le worker utilise `async/await` — une façon de faire plusieurs choses "en même temps" sans bloquer.

> **Synchrone** : tu fais une action, tu attends qu'elle finisse, tu fais la suivante.
> **Asynchrone** : tu lances une action, et pendant qu'elle s'exécute, tu peux faire autre chose. Quand elle finit, tu reprends là où tu en étais.

En pratique dans notre code : quand le worker attend un message de Google Pub/Sub (opération réseau qui peut prendre du temps), il ne "dort" pas et ne bloque pas le CPU. Il attend de façon non-bloquante.

---

## 3. Les fondamentaux Rust utilisés ici

Avant d'expliquer les fichiers, voici les concepts Rust qui apparaissent partout.

### `struct` — Regrouper des données liées

> Équivalent d'une **classe** en Java/Python, mais sans héritage. C'est un "moule" pour créer des objets avec des champs nommés.

```rust
pub struct Config {
    pub gcp_project_id: String,       // l'ID du projet Google Cloud
    pub subscription_demands: String, // le nom de la queue des demandes
    pub topic_responses: String,      // le nom de la queue des réponses
}
```

On accède aux champs avec `.` : `config.gcp_project_id`

### `enum` — Un choix parmi plusieurs cas

> Comme un menu à choix fixe. La valeur ne peut être **que l'un** des cas listés.

```rust
pub enum ResponseStatus {
    Success,  // la tâche a réussi
    Error,    // la tâche a échoué
}
```

Les `enum` Rust sont puissants : chaque cas peut contenir des données :

```rust
pub enum WorkerError {
    MissingColumn(String),  // erreur avec le nom de la colonne manquante
    ExcelError(String),     // erreur avec le message d'explication
    // ...
}
```

### `impl` — Ajouter des méthodes à une struct ou enum

> Comme les méthodes d'une classe. On "implémente" des fonctions liées à un type.

```rust
impl Config {
    pub fn from_env() -> Result<Self, WorkerError> { ... }
}
```

On appelle ensuite : `Config::from_env()`

### `trait` — Définir un contrat (interface)

> Équivalent d'une **interface** en Java. Un trait dit "tout type qui l'implémente DOIT avoir ces méthodes".

```rust
pub trait SchemaParser {
    fn expected_columns(&self) -> &'static [&'static str];
    fn parse_row(&self, ...) -> Result<...>;
}
```

`FootballParser` et `EsportParser` implémentent ce trait — ils s'engagent à avoir ces deux méthodes.

### `Result<T, E>` — Gérer les erreurs sans exceptions

> Au lieu de lancer des exceptions (Java, Python), Rust force à gérer les erreurs explicitement.
> `Result<T, E>` = soit `Ok(valeur)` (succès), soit `Err(erreur)` (échec).

```rust
fn from_env() -> Result<Config, WorkerError>
// Retourne soit Ok(config) soit Err(WorkerError::ConfigError(...))
```

L'opérateur `?` propage l'erreur automatiquement :
```rust
let config = Config::from_env()?;
// Si from_env() retourne Err, la fonction courante retourne immédiatement cette erreur
```

### `Option<T>` — Une valeur qui peut être absente

> Équivalent de `null` en Java, mais **sûr** : le compilateur force à gérer le cas "absent".
> `Option<T>` = soit `Some(valeur)`, soit `None`.

```rust
pub data: Option<serde_json::Value>,
// Le champ data peut être présent (Some) ou absent (None)
```

### `async fn` et `await` — Code non-bloquant

```rust
pub async fn pull(&self) -> Result<Vec<ReceivedMessage>, WorkerError> {
    self.subscription.pull(10, None).await // .await attend le résultat sans bloquer
}
```

---

## 4. Carte de tous les fichiers

```
worker/
├── Cargo.toml                  ← Liste des dépendances (comme package.json en Node.js)
├── .env.example                ← Exemple de variables d'environnement à configurer
└── src/
    ├── main.rs                 ← Point d'entrée du programme
    ├── config.rs               ← Chargement de la configuration
    ├── errors.rs               ← Tous les types d'erreurs
    ├── models.rs               ← Structures des messages JSON
    │
    ├── queue/
    │   ├── mod.rs              ← Rend public consumer et producer
    │   ├── consumer.rs         ← Reçoit les messages de Pub/Sub
    │   └── producer.rs         ← Envoie les réponses à Pub/Sub
    │
    ├── tasks/
    │   ├── mod.rs              ← Rend public dispatcher et import_excel
    │   ├── dispatcher.rs       ← Choisit quelle tâche exécuter
    │   └── import_excel.rs     ← La tâche : lire un Excel et extraire les joueurs
    │
    ├── parser/
    │   ├── mod.rs              ← Choisit le bon parser selon le type de tournoi
    │   ├── traits.rs           ← Contrat SchemaParser + fonction utilitaire cell_str
    │   ├── football.rs         ← Parser Football 11v11
    │   └── esport.rs           ← Parser Esport 5v5
    │
    └── retry/
        └── mod.rs              ← Logique de retry avec backoff exponentiel
```

---

## 5. Fichier par fichier — explication complète

---

### `Cargo.toml` — Le fichier de configuration du projet

> Équivalent de `package.json` en Node.js ou `pom.xml` en Java. Il déclare le nom du projet, sa version, et toutes les bibliothèques externes utilisées.

**Dépendances déclarées et leur rôle :**

| Bibliothèque | Rôle |
|---|---|
| `tokio` | Runtime asynchrone — permet d'utiliser `async/await` |
| `serde` + `serde_json` | Sérialisation/désérialisation JSON |
| `google-cloud-pubsub` | Client Rust pour Google Cloud Pub/Sub |
| `google-cloud-googleapis` | Types protobuf de Google (dont `PubsubMessage`) |
| `calamine` | Lecture de fichiers Excel (.xlsx) |
| `base64` | Décodage de données encodées en base64 |
| `thiserror` | Macro pour créer des types d'erreurs propres |
| `anyhow` | Gestion d'erreurs simplifiée dans `main` |
| `tracing` + `tracing-subscriber` | Système de logs structurés |
| `dotenvy` | Chargement du fichier `.env` |
| `tokio-util` | Utilitaires Tokio (dont `CancellationToken`) |

---

### `src/config.rs` — Configuration

**Rôle :** Lire les variables d'environnement au démarrage et les regrouper dans une structure.

#### Pourquoi une `struct Config` ?

Les variables d'environnement sont des `String` éparpillées dans le système. En les regroupant dans une `struct`, on :
- Valide qu'elles existent toutes **une seule fois** au démarrage
- Passe un seul objet `config` au lieu de 3 variables séparées
- Obtient une erreur claire si une variable est manquante

```rust
pub struct Config {
    pub gcp_project_id: String,        // ex: "mon-projet-gcp"
    pub subscription_demands: String,  // ex: "projects/mon-projet/subscriptions/sub-demandes"
    pub topic_responses: String,       // ex: "projects/mon-projet/topics/topic-reponses"
}
```

#### Fonction `Config::from_env()`

```rust
pub fn from_env() -> Result<Self, WorkerError>
```

**Ce qu'elle fait :**
1. Appelle `dotenvy::dotenv().ok()` → charge le fichier `.env` s'il existe
2. Lit chaque variable avec `std::env::var("NOM_VARIABLE")`
3. Si une variable est absente → retourne `Err(WorkerError::ConfigError(...))`
4. Si tout est ok → retourne `Ok(Config { ... })`

> **Variable d'environnement** : une variable configurée au niveau du système d'exploitation (ou dans un fichier `.env`), pas dans le code. Permet de ne pas mettre les mots de passe dans le code source.

---

### `src/errors.rs` — Types d'erreurs

**Rôle :** Centraliser tous les types d'erreurs possibles dans le worker en un seul `enum`.

#### Pourquoi un `enum` pour les erreurs ?

En Rust, il n'y a pas d'exceptions. Chaque erreur est une valeur. Un `enum` permet de lister **exhaustivement** tous les cas d'erreur possibles. Le compilateur force ensuite à les gérer tous.

```rust
pub enum WorkerError {
    MissingColumn(String),     // une colonne Excel est absente — contient son nom
    ParseError(String),        // erreur de parsing — contient le message
    InvalidBase64(DecodeError), // base64 invalide — contient l'erreur de la lib
    ExcelError(String),        // erreur Excel — contient le message
    JsonError(serde_json::Error), // erreur JSON — contient l'erreur de serde
    QueueError(String),        // erreur Pub/Sub — contient le message
    UnknownTaskType(String),   // type de tâche inconnu — contient le nom reçu
    InvalidPayload(String),    // payload JSON mal formé — contient le message
    ConfigError(String),       // variable d'env manquante — contient le message
}
```

Le `#[derive(Debug, Error)]` au-dessus est une **macro dérivée** :
- `Debug` : permet d'afficher l'erreur avec `{:?}` dans les logs
- `Error` (de `thiserror`) : génère automatiquement les messages d'erreur définis par `#[error("...")]`

Le `#[from]` sur certains variants (ex: `InvalidBase64`) permet la conversion automatique :
```rust
// Au lieu d'écrire WorkerError::InvalidBase64(e) manuellement,
// le ? opérateur fait la conversion tout seul
let bytes = STANDARD.decode(&file_base64)?; // si ça échoue → WorkerError::InvalidBase64
```

#### Fonction `error_code(&self) -> &'static str`

Retourne un code d'erreur en majuscules pour le JSON de réponse.

```rust
WorkerError::MissingColumn(_) => "INVALID_SCHEMA"
WorkerError::ExcelError(_)    => "EXCEL_ERROR"
// etc.
```

> `&'static str` signifie une chaîne de caractères "statique" — elle existe pour toute la durée du programme et ne sera jamais désallouée. C'est le type des chaînes littérales comme `"INVALID_SCHEMA"`.

---

### `src/models.rs` — Structures de données des messages

**Rôle :** Définir la forme exacte des messages JSON qui circulent entre le backend et le worker.

#### `struct IncomingMessage` — Message reçu du backend

```rust
pub struct IncomingMessage {
    pub task_id: String,            // ex: "a1b2c3d4-..."  — identifiant unique
    pub task_type: String,          // ex: "import_excel"
    pub payload: serde_json::Value, // contenu variable selon le type de tâche
}
```

Corresponds au JSON :
```json
{
  "task_id": "a1b2c3d4-e5f6-...",
  "task_type": "import_excel",
  "payload": {
    "tournament_type": "football_11v11",
    "file_base64": "UEsDBBQA..."
  }
}
```

`serde_json::Value` est un type "dynamique" — il peut représenter n'importe quelle valeur JSON (objet, tableau, string, nombre). On l'utilise pour `payload` car son contenu change selon le `task_type`.

#### `struct WorkerResponse` — Réponse publiée par le worker

```rust
pub struct WorkerResponse {
    pub task_id: String,
    pub task_type: String,
    pub status: ResponseStatus,
    pub data: Option<serde_json::Value>,   // présent si succès, absent si erreur
    pub error: Option<ErrorDetail>,        // présent si erreur, absent si succès
}
```

`#[serde(skip_serializing_if = "Option::is_none")]` : n'inclut pas le champ dans le JSON s'il est `None`. Ainsi le JSON de succès n'a pas de champ `"error"`, et vice-versa.

#### `enum ResponseStatus`

```rust
pub enum ResponseStatus {
    Success,  // → sérialisé en "success" dans le JSON (grâce à rename_all = "lowercase")
    Error,    // → sérialisé en "error"
}
```

`#[serde(rename_all = "lowercase")]` dit à serde de convertir les noms de variants en minuscules lors de la sérialisation JSON.

#### `struct ErrorDetail`

```rust
pub struct ErrorDetail {
    pub code: String,      // ex: "INVALID_SCHEMA"
    pub message: String,   // ex: "Colonne 'Poste' manquante"
    pub attempts: u32,     // nombre de tentatives effectuées
}
```

#### Fonctions `WorkerResponse::success(...)` et `WorkerResponse::error(...)`

Ce sont des **constructeurs statiques** — des fonctions associées à la struct (pas des méthodes d'instance) qui créent un `WorkerResponse` pré-rempli.

```rust
// Construire une réponse succès :
WorkerResponse::success("id-123".to_string(), "import_excel".to_string(), json_data)
// → { task_id: "id-123", task_type: "import_excel", status: Success, data: Some(...), error: None }

// Construire une réponse erreur :
WorkerResponse::error("id-123".to_string(), "import_excel".to_string(), "EXCEL_ERROR".to_string(), "Fichier corrompu".to_string(), 3)
// → { task_id: "id-123", ..., status: Error, data: None, error: Some(ErrorDetail { ... }) }
```

---

### `src/retry/mod.rs` — Politique de retry

**Rôle :** Réessayer automatiquement une tâche qui échoue, avec un délai croissant entre les tentatives.

#### Pourquoi retenter ?

Un fichier Excel peut échouer à parser à cause d'un problème réseau temporaire ou d'un pic de charge. Plutôt que de déclarer immédiatement l'échec, on réessaie. Mais si on réessaie immédiatement en boucle, on risque de surcharger le système. D'où le **backoff exponentiel** : on attend de plus en plus longtemps entre chaque tentative.

#### `struct RetryPolicy`

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,    // nombre maximum de tentatives
    pub initial_delay: Duration, // délai de base (le premier vrai délai)
}
```

#### Fonction `RetryPolicy::for_task(task_type: &str) -> Self`

Constructeur statique qui retourne la politique adaptée à chaque type de tâche :

```rust
match task_type {
    "import_excel" => Self { max_attempts: 3, initial_delay: Duration::from_secs(1) },
    _              => Self { max_attempts: 1, initial_delay: Duration::ZERO },
}
```

#### Fonction privée `backoff_delay(attempt: u32) -> Duration`

Calcule le délai avant la tentative numéro `attempt` :

| Tentative (attempt) | Calcul | Délai |
|---|---|---|
| 0 (première) | — | 0 secondes (pas d'attente) |
| 1 | 1s × 2⁰ | 1 seconde |
| 2 | 1s × 2¹ | 2 secondes |
| 3 | 1s × 2² | 4 secondes |

```rust
fn backoff_delay(&self, attempt: u32) -> Duration {
    if attempt == 0 { return Duration::ZERO; }
    self.initial_delay * 2u32.pow(attempt - 1)
}
```

> **Backoff exponentiel** : stratégie où l'attente double à chaque échec. Très utilisée dans les systèmes distribués pour éviter les "avalanches" de requêtes quand un service est surchargé.

#### Fonction `execute_with_retry<F, Fut>(policy, f) -> Result<Value, (WorkerError, u32)>`

C'est la fonction principale du module. Elle est **générique** (les `<F, Fut>` sont des paramètres de type).

```rust
pub async fn execute_with_retry<F, Fut>(
    policy: &RetryPolicy, // la politique (nb de tentatives, délai)
    mut f: F,             // la fonction à exécuter (une "closure" qui retourne un Future)
) -> Result<serde_json::Value, (WorkerError, u32)>
```

> **Closure** : une fonction anonyme qu'on passe en argument à une autre fonction. Comme un callback en JavaScript.

> **Generic / Générique** : une fonction qui fonctionne avec n'importe quel type, tant qu'il respecte certaines contraintes. `F: FnMut() -> Fut` signifie "F est quelque chose qu'on peut appeler comme une fonction sans argument et qui retourne un Future".

**Ce qu'elle fait :**
```
Pour chaque tentative de 0 à max_attempts :
  1. Calculer et attendre le délai backoff
  2. Appeler f() pour obtenir un Future
  3. Attendre le résultat de ce Future
     → Si Ok(data) : retourner Ok(data) immédiatement
     → Si Err(e) : logger l'erreur, mémoriser l'erreur, continuer
Si toutes les tentatives ont échoué :
  → Si au moins une erreur a eu lieu : Retourner Err((dernière_erreur, nb_tentatives))
  → Si max_attempts était 0 (boucle jamais exécutée) : Retourner une erreur de config
```

Le cas `max_attempts = 0` est protégé par un `match` sur `last_error` pour éviter un crash (`unwrap()` sur `None` provoquerait un panic en Rust).

---

### `src/parser/traits.rs` — Contrat et utilitaire Excel

**Rôle :** Définir l'interface commune à tous les parsers de schéma Excel, et fournir une fonction utilitaire partagée.

#### Fonction libre `cell_str(row, col_index, col) -> Result<String, WorkerError>`

> **Fonction libre** : une fonction qui n'appartient ni à une struct ni à un trait — elle existe seule dans le module. En Rust c'est normal et courant.

Elle prend :
- `row: &[Data]` : un tableau de cellules Excel représentant une ligne
- `col_index: &HashMap<String, usize>` : une map "nom de colonne" → "numéro de colonne"
- `col: &str` : le nom de la colonne qu'on veut lire

Elle retourne la valeur de la cellule convertie en `String`.

```rust
// Exemple : lire la colonne "Nom" sur la ligne 3 du fichier
let nom = cell_str(row, &col_index, "Nom")?;
```

Le `match` interne gère les différents types de cellule que peut contenir un Excel :
```rust
match cell {
    Data::String(s) => s.trim().to_string(),       // texte → retirer les espaces
    Data::Float(f)  => ...                         // nombre décimal → "42.5" ou "42"
    Data::Int(i)    => i.to_string(),              // entier → "7"
    Data::Bool(b)   => b.to_string(),              // booléen → "true" / "false"
    Data::Empty     => String::new(),              // cellule vide → ""
    other           => other.to_string(),          // autre → conversion générique
}
```

> `Data` est l'`enum` de la bibliothèque `calamine` qui représente le contenu possible d'une cellule Excel. Chaque variant contient la valeur du bon type.

#### Pourquoi `cell_str` est-elle EN DEHORS du trait ?

En Rust, un trait ne peut être utilisé comme "objet dynamique" (`Box<dyn SchemaParser>`) que si **toutes ses méthodes prennent `&self`**. Une fonction statique (sans `self`) casse cette propriété appelée **dyn compatibility**.

En mettant `cell_str` comme fonction libre, le trait reste compatible avec `Box<dyn SchemaParser>`, ce qui permet d'avoir `FootballParser` ou `EsportParser` derrière le même pointeur.

#### `trait SchemaParser`

```rust
pub trait SchemaParser: Send + Sync {
    fn expected_columns(&self) -> &'static [&'static str];
    fn parse_row(&self, row: &[Data], col_index: &HashMap<String, usize>) -> Result<serde_json::Value, WorkerError>;
}
```

- `Send + Sync` : contraintes nécessaires pour utiliser ce trait dans du code asynchrone multi-thread. `Send` = peut être envoyé entre threads. `Sync` = peut être accédé simultanément depuis plusieurs threads.
- `expected_columns` : retourne la liste des noms de colonnes que le fichier Excel DOIT avoir
- `parse_row` : transforme une ligne brute Excel en JSON structuré

---

### `src/parser/football.rs` — Parser Football 11v11

**Rôle :** Implémenter `SchemaParser` pour les tournois de football.

```rust
pub struct FootballParser;  // struct vide : elle n'a pas de données, juste des méthodes
```

> Une **struct vide** en Rust est valide. Elle sert juste de "type" pour regrouper des méthodes. C'est utile quand on veut implémenter un trait sans avoir besoin de stocker d'état.

```rust
impl SchemaParser for FootballParser {
    fn expected_columns(&self) -> &'static [&'static str] {
        &["Nom", "Prénom", "Poste", "Numéro"]  // colonnes obligatoires
    }

    fn parse_row(&self, row, col_index) -> Result<Value, WorkerError> {
        Ok(json!({
            "last_name":  cell_str(row, col_index, "Nom")?,
            "first_name": cell_str(row, col_index, "Prénom")?,
            "position":   cell_str(row, col_index, "Poste")?,
            "number":     cell_str(row, col_index, "Numéro")?,
        }))
    }
}
```

La macro `json!({...})` crée directement un objet JSON depuis du code Rust.

---

### `src/parser/esport.rs` — Parser Esport 5v5

Même principe que football, mais avec les colonnes `Pseudo`, `Équipe`, `Rang`, et une structure de sortie JSON différente (`username`, `team`, `rank`).

---

### `src/parser/mod.rs` — Sélection du bon parser

**Rôle :** Choisir quel parser instancier selon le type de tournoi reçu dans le message.

#### Fonction `get_parser(tournament_type: &str) -> Result<Box<dyn SchemaParser>, WorkerError>`

```rust
pub fn get_parser(tournament_type: &str) -> Result<Box<dyn SchemaParser>, WorkerError> {
    match tournament_type {
        "football_11v11" => Ok(Box::new(FootballParser)),
        "esport_5v5"     => Ok(Box::new(EsportParser)),
        other            => Err(WorkerError::ParseError(format!("Unknown: {other}"))),
    }
}
```

**Pourquoi `Box<dyn SchemaParser>` ?**

- `dyn SchemaParser` : "quelque chose qui implémente `SchemaParser`" — le type exact est inconnu à la compilation
- `Box<...>` : alloue la valeur sur le **heap** (tas mémoire) et retourne un pointeur dessus

> **Stack vs Heap** : la stack (pile) est une mémoire rapide mais de taille fixe. Le heap (tas) est une mémoire plus grande où on peut allouer des objets dont la taille est inconnue à la compilation. `Box` = "mets cet objet sur le heap et donne-moi un pointeur".

En résumé : `Box<dyn SchemaParser>` = "un pointeur vers n'importe quel parser, peu importe lequel". Ça permet d'avoir `FootballParser` ou `EsportParser` derrière la même variable sans que le reste du code ait besoin de savoir lequel c'est.

**Pour ajouter un nouveau type de tournoi :**
1. Créer `src/parser/tennis.rs` avec `TennisParser`
2. Ajouter `"tennis_simple" => Ok(Box::new(TennisParser))` dans ce `match`

---

### `src/tasks/import_excel.rs` — La tâche principale

**Rôle :** Implémenter la logique complète du traitement d'un fichier Excel uploadé.

#### `struct ImportExcelPayload` (privée au module)

```rust
struct ImportExcelPayload {
    tournament_type: String, // ex: "football_11v11"
    file_base64: String,     // le contenu du fichier Excel encodé en base64
}
```

C'est une struct privée (pas de `pub`) car elle n'est utilisée qu'à l'intérieur de ce fichier pour désérialiser le payload JSON.

#### Fonction `execute(payload: serde_json::Value) -> Result<serde_json::Value, WorkerError>`

C'est la fonction publique appelée par le dispatcher. Voici les étapes :

**Étape 1 : Désérialiser le payload**
```rust
let payload: ImportExcelPayload = serde_json::from_value(payload)?;
```
Le JSON brut `{ "tournament_type": "...", "file_base64": "..." }` est converti en struct Rust.

**Étape 2 : Décoder le base64**
```rust
let bytes = STANDARD.decode(&payload.file_base64)?;
```
> **Base64** : un encodage qui transforme des données binaires (comme un fichier Excel) en une chaîne de texte ne contenant que des caractères "sûrs" (A-Z, a-z, 0-9, +, /). Utile pour transporter des fichiers dans du JSON. `decode` fait l'inverse : retransforme le texte en bytes.

**Étape 3 : Ouvrir le fichier Excel depuis la mémoire**
```rust
let cursor = Cursor::new(bytes);
let mut workbook = open_workbook_auto_from_rs(cursor)?;
```
> `Cursor` : un wrapper qui fait croire à calamine qu'il lit depuis un fichier sur disque, alors qu'il lit depuis la mémoire. Calamine veut quelque chose qui implémente `Read + Seek` (lire et se déplacer dans les données). `Cursor<Vec<u8>>` (octets en mémoire) l'implémente.

**Étape 4 : Lire le premier onglet**
```rust
let sheet_names = workbook.sheet_names().to_vec();
let sheet_name = sheet_names.first()...?;
let range = workbook.worksheet_range(&sheet_name)?;
```
`range` contient toutes les cellules de l'onglet sous forme de tableau 2D.

**Étape 5 : Récupérer le bon parser**
```rust
let parser = parser::get_parser(&payload.tournament_type)?;
```

**Étape 6 : Lire les en-têtes (première ligne)**
```rust
let mut rows = range.rows();
let headers: Vec<String> = rows.next()...collect();
```
On prend la première ligne de l'itérateur et on convertit chaque cellule en `String`.

**Étape 7 : Valider que les colonnes attendues sont présentes**
```rust
for expected in parser.expected_columns() {
    if !headers.iter().any(|h| h == expected) {
        return Err(WorkerError::MissingColumn(expected.to_string()));
    }
}
```
Si le fichier football n'a pas la colonne "Poste", on retourne immédiatement une erreur.

**Étape 8 : Construire l'index des colonnes**
```rust
let col_index: HashMap<String, usize> = headers.into_iter().enumerate().map(|(i, h)| (h, i)).collect();
```
> `HashMap<String, usize>` : une table de hachage (dictionnaire) qui associe un nom de colonne à son numéro. Ex: `{ "Nom" → 0, "Prénom" → 1, "Poste" → 2, "Numéro" → 3 }`.

Cet index est indispensable car l'ordre des colonnes dans l'Excel peut changer. On cherche les colonnes par **nom**, pas par position.

**Étape 9 : Parser chaque ligne**
```rust
let players: Result<Vec<Value>, WorkerError> = rows
    .filter(|row| row.iter().any(|c| !matches!(c, Data::Empty)))  // ignorer lignes vides
    .map(|row| parser.parse_row(row, &col_index))
    .collect();
```
`.filter(...)` ignore les lignes complètement vides.
`.map(...)` transforme chaque ligne en JSON via le parser.
`.collect()` assemble tous les JSON dans un `Vec`. Si **une** ligne échoue, le `Result` entier devient `Err`.

**Étape 10 : Retourner le résultat**
```rust
Ok(serde_json::json!({ "players": players? }))
```
Retourne `{ "players": [ {...}, {...}, ... ] }`.

---

### `src/tasks/dispatcher.rs` — Aiguillage des tâches

**Rôle :** Recevoir un message, trouver quelle tâche exécuter, la lancer avec retry, retourner une réponse.

#### Fonction `dispatch(msg: &IncomingMessage) -> WorkerResponse`

```rust
pub async fn dispatch(msg: &IncomingMessage) -> WorkerResponse {
```

C'est la fonction centrale du worker. Elle ne retourne jamais d'erreur directement — elle retourne toujours un `WorkerResponse` (succès ou erreur structurée). Ainsi le backend reçoit toujours une réponse.

**Étape 1 : Créer la politique de retry adaptée**
```rust
let policy = RetryPolicy::for_task(&msg.task_type);
```

**Étape 2 : Exécuter la tâche avec retry**
```rust
let result = execute_with_retry(&policy, || {
    let task_type = msg.task_type.clone();
    let payload = msg.payload.clone();
    async move {
        match task_type.as_str() {
            "import_excel" => import_excel::execute(payload).await,
            other          => Err(WorkerError::UnknownTaskType(other.to_string())),
        }
    }
}).await;
```

La closure passée à `execute_with_retry` est recréée à chaque tentative. Elle clone `task_type` et `payload` pour les capturer dans le `async move`. Le `async move` est nécessaire pour que le compilateur sache que les valeurs clonées appartiennent au bloc asynchrone.

**Étape 3 : Construire la réponse**
```rust
match result {
    Ok(data) => WorkerResponse::success(..., data),
    Err((e, attempts)) => WorkerResponse::error(..., e.error_code(), e.to_string(), attempts),
}
```

---

### `src/queue/consumer.rs` — Réception des messages

**Rôle :** Abstraire la connexion à Google Pub/Sub pour la réception de messages.

#### `struct QueueConsumer`

```rust
pub struct QueueConsumer {
    subscription: Subscription,  // connexion à la subscription Pub/Sub
}
```

#### `QueueConsumer::new(subscription: Subscription) -> Self`

Constructeur simple : prend une connexion Pub/Sub existante et la stocke.

#### `pull(&self, max: i32) -> Result<Vec<ReceivedMessage>, WorkerError>`

Appelle `subscription.pull(max, None)` pour demander jusqu'à `max` messages à Pub/Sub.

- Si des messages sont disponibles → retourne un `Vec<ReceivedMessage>`
- Si la queue est vide → retourne `Ok(vec![])` (vecteur vide)
- Si erreur réseau → retourne `Err(WorkerError::QueueError(...))`

> `Vec<T>` : un tableau dynamique en Rust (comme un `ArrayList` en Java ou une liste en Python).

---

### `src/queue/producer.rs` — Envoi des réponses

**Rôle :** Abstraire l'envoi de messages vers Google Pub/Sub.

#### `struct QueueProducer`

```rust
pub struct QueueProducer {
    publisher: Publisher,  // objet de la lib google-cloud-pubsub
}
```

#### `QueueProducer::new(topic: Topic) -> Self`

```rust
Self { publisher: topic.new_publisher(None) }
```

Crée un `Publisher` depuis un `Topic`. Ce `Publisher` s'occupe du **batching** (regrouper plusieurs messages pour les envoyer en une seule requête HTTP) et de la mise en file d'attente interne.

#### `publish(&self, response: &WorkerResponse) -> Result<(), WorkerError>`

1. Convertit `WorkerResponse` en JSON : `serde_json::to_string(response)`
2. Convertit le JSON en `Vec<u8>` (octets) : `json.into_bytes()`
3. Crée un `PubsubMessage` avec ces octets
4. Publie via `self.publisher.publish(msg).await`
5. Attend la confirmation avec `.get().await`

> `PubsubMessage` est le type défini par les APIs Google (via protobuf). Son champ `data` contient les octets bruts du message. Le champ `..Default::default()` remplit tous les autres champs avec leurs valeurs par défaut (vide pour `attributes`, etc.).

---

### `src/main.rs` — Le cœur du programme

**Rôle :** Point d'entrée. Initialise tout, lance la boucle principale, gère l'arrêt propre.

#### `#[tokio::main]`

C'est une **macro attribut** qui transforme la fonction `main` synchrone en une fonction asynchrone avec le runtime Tokio. Sans ça, `async fn main` n'existerait pas en Rust standard.

#### Initialisation des logs

```rust
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

Configure le système de logs. `from_default_env()` lit la variable `RUST_LOG` pour choisir le niveau (`info`, `debug`, etc.).

#### Connexion à Google Pub/Sub

```rust
let gcp_config = ClientConfig::default().with_auth().await?;
let client = Client::new(gcp_config).await?;
let subscription = client.subscription(&config.subscription_demands);
let topic = client.topic(&config.topic_responses);
```

`.with_auth()` utilise les **Application Default Credentials** de Google : il cherche les credentials dans l'ordre — variable d'env `GOOGLE_APPLICATION_CREDENTIALS`, puis compte de service GCP, puis instance GCP.

#### Graceful shutdown avec `CancellationToken`

```rust
let shutdown = CancellationToken::new();
```

> **Graceful shutdown** (arrêt propre) : quand le programme reçoit l'ordre de s'arrêter (Ctrl+C ou signal système), il finit la tâche en cours avant de s'arrêter. Il ne coupe pas brutalement au milieu du traitement.

Un `CancellationToken` est comme un **interrupteur partagé** :
- On peut en créer des copies avec `.clone()`
- Quand on appelle `.cancel()` sur une copie, TOUTES les copies voient le changement
- On peut attendre qu'il soit annulé avec `.cancelled().await`

Deux tâches asynchrones écoutent les signaux système :
- `tokio::signal::ctrl_c()` → Ctrl+C (SIGINT)
- `signal(SignalKind::terminate())` → SIGTERM (signal envoyé par Docker/Kubernetes pour arrêter un container)

Quand l'un d'eux se déclenche → `shutdown_ctrlc.cancel()` → le token est annulé.

#### La boucle principale

```rust
loop {
    tokio::select! {
        _ = shutdown.cancelled() => { break; }

        result = consumer.pull(10) => {
            match result {
                Ok(messages) if messages.is_empty() => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Ok(mut messages) => {
                    for msg in &mut messages {
                        // traiter le message
                    }
                }
                Err(e) => {
                    // logger l'erreur, attendre 1s
                }
            }
        }
    }
}
```

> `tokio::select!` : attend **plusieurs futures en parallèle** et réagit dès que l'une d'elles se termine. Ici : soit le token de shutdown est annulé (→ on sort de la boucle), soit `consumer.pull(10)` retourne des messages (→ on les traite). Le premier qui arrive "gagne".

**Traitement d'un message individuel :**

1. `msg.message.data.clone()` → récupère les octets bruts du message
2. `serde_json::from_slice::<IncomingMessage>(&data)` → parse le JSON en struct Rust
3. `dispatcher::dispatch(&incoming).await` → exécute la tâche (avec retry intégré)
4. `producer.publish(&response).await` → publie la réponse dans topic-réponses
5. `msg.ack().await` → confirme à Pub/Sub que le message a été traité

> **ACK (acknowledgement)** : le worker dit à Pub/Sub "j'ai bien reçu et traité ce message, tu peux le supprimer". Si le worker ne fait pas d'ACK dans un certain délai, Pub/Sub relivre le message à un autre worker (mécanisme de sécurité intégré).

L'ACK est toujours fait, même si la tâche a échoué — car le retry est géré en interne par `execute_with_retry`. On ne veut pas que Pub/Sub relivr le message infiniment.

---

## 6. Pourquoi `struct`, `enum`, `trait` ? Le choix expliqué

| Situation | Structure choisie | Pourquoi |
|---|---|---|
| Regrouper la configuration | `struct Config` | Les données sont fixes et nommées. On y accède par `config.gcp_project_id`. |
| Représenter une liste d'erreurs | `enum WorkerError` | Chaque erreur est un cas différent. L'enum garantit qu'on gère tous les cas (le compilateur vérifie). |
| Représenter un état binaire (succès/erreur) | `enum ResponseStatus` | Seulement 2 états possibles, mutuellement exclusifs. Un bool serait moins lisible. |
| Définir un contrat commun aux parsers | `trait SchemaParser` | Permet d'avoir plusieurs parsers interchangeables derrière le même type (`Box<dyn SchemaParser>`). |
| Parser sans état interne | `struct FootballParser` (vide) | On a besoin d'un type pour implémenter le trait, mais pas de données. Une struct vide est la solution idiomatique en Rust. |
| Politique de retry paramétrable | `struct RetryPolicy` | Les paramètres (`max_attempts`, `initial_delay`) varient selon la tâche. Mieux qu'une série de constantes globales. |
| Messages JSON entrants/sortants | `struct IncomingMessage`, `struct WorkerResponse` | Les champs sont connus à la compilation. Serde peut faire la sérialisation/désérialisation automatiquement grâce à `#[derive]`. |

---

## 7. Le flux complet d'un message de A à Z

Voici ce qui se passe exactement quand le backend envoie un fichier Excel à importer :

```
① Backend publie ce JSON dans "topic-demandes" :
   {
     "task_id": "abc-123",
     "task_type": "import_excel",
     "payload": {
       "tournament_type": "football_11v11",
       "file_base64": "UEsDB..."
     }
   }

② main.rs — consumer.pull(10)
   Pub/Sub retourne ce message au worker

③ main.rs — serde_json::from_slice::<IncomingMessage>(&data)
   Désérialisation JSON → struct IncomingMessage {
     task_id: "abc-123",
     task_type: "import_excel",
     payload: { ... }
   }

④ dispatcher::dispatch(&incoming)

   ④a. RetryPolicy::for_task("import_excel")
        → max_attempts: 3, initial_delay: 1s

   ④b. execute_with_retry(policy, closure)
        → Tentative 1 (délai: 0s) :
          ④c. import_excel::execute(payload)
               - Décode base64 → bytes
               - Ouvre le classeur Excel en mémoire
               - Lit le premier onglet
               - get_parser("football_11v11") → FootballParser
               - Lit la ligne d'en-têtes
               - Valide: "Nom", "Prénom", "Poste", "Numéro" présents ?
               - Construit col_index: {"Nom":0, "Prénom":1, ...}
               - Parse chaque ligne → [ {last_name, first_name, position, number}, ... ]
               - Retourne Ok({ "players": [...] })
          ← Ok(data)
        → execute_with_retry retourne Ok(data)

   ④d. WorkerResponse::success("abc-123", "import_excel", data)
        → { task_id: "abc-123", task_type: "import_excel", status: Success, data: {...} }

⑤ producer.publish(&response)
   Sérialise en JSON, publie dans "topic-réponses"

⑥ msg.ack()
   Confirme à Pub/Sub que le message est traité

⑦ Backend lit "topic-réponses", trouve la réponse avec task_id "abc-123",
   insère les joueurs en base de données PostgreSQL
```

---

## 8. Les dépendances (bibliothèques externes)

### `tokio` — Le moteur asynchrone

Tokio est le **runtime** qui fait tourner le code `async/await`. Sans lui, le code asynchrone ne s'exécuterait pas. La macro `#[tokio::main]` transforme le `main` en "boucle d'événements" qui orchestre toutes les tâches async.

### `serde` + `serde_json`

`serde` est le framework de sérialisation de Rust. Avec `#[derive(Serialize, Deserialize)]`, il génère automatiquement le code de conversion JSON ↔ struct. Aucun code de parsing JSON n'est écrit à la main.

### `google-cloud-pubsub`

Client Rust officiel pour Google Cloud Pub/Sub. Il gère la connexion HTTPS, l'authentification OAuth2, le batching des messages, et les retransmissions au niveau réseau.

### `google-cloud-googleapis`

Contient les types générés depuis les fichiers protobuf de Google. `PubsubMessage` en fait partie.

> **Protobuf** : format de sérialisation binaire de Google. Les types Rust sont générés automatiquement depuis des fichiers `.proto` (définitions des APIs Google).

### `calamine`

Bibliothèque Rust pour lire les fichiers Excel (`.xlsx`, `.xls`, `.ods`). Elle peut lire depuis la mémoire grâce à `open_workbook_auto_from_rs`. Le type `Data` représente le contenu d'une cellule.

### `base64`

Encodage/décodage base64. `STANDARD.decode(string)` transforme une chaîne base64 en `Vec<u8>`.

### `thiserror`

Macro qui simplifie la création d'enums d'erreurs. `#[error("Missing column: {0}")]` génère automatiquement l'implémentation du trait `std::error::Error` et le message d'affichage.

### `tracing` + `tracing-subscriber`

Système de logs structurés. `tracing::info!(key = %value, "message")` produit des logs avec des métadonnées clés/valeurs, bien meilleurs que de simples `println!` pour la production.

### `dotenvy`

Charge un fichier `.env` dans les variables d'environnement au démarrage. Pratique pour le développement local.

### `tokio-util` — `CancellationToken`

Utilitaire Tokio pour la coordination d'arrêt entre plusieurs tâches asynchrones. Le `CancellationToken` est un "drapeau" partagé et thread-safe.

---

*Document généré le 2026-06-08 — Worker Rust BracketHub v0.1.0*
