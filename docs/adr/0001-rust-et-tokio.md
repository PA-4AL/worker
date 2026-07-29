# ADR-0001 — Rust et Tokio pour le worker

- **Date** : 2026-01-29
- **Statut** : accepté
- **Portée** : worker

## Contexte

Le worker traite des fichiers Excel de façon asynchrone. Il doit tourner en
permanence pour consommer une file, avec une empreinte mémoire faible puisqu'il
est facturé à la durée d'exécution. Le brief accorde des points supplémentaires
pour un worker en Rust.

## Décision

**Rust** avec l'ordonnanceur asynchrone **Tokio**.

Écarté : réutiliser **Kotlin/JVM** — cela aurait mutualisé les compétences, mais
la JVM impose plusieurs centaines de mégaoctets de mémoire résidente et un
démarrage lent pour un processus censé rester allumé en permanence.

## Conséquences

- image finale de **33 Mo** (binaire sur base distroless), démarrage immédiat
- aucun modèle partagé possible avec le backend : le contrat de messages devient
  explicite, ce qui va dans le sens de l'isolation exigée
- l'absence de partage de code impose de verrouiller le contrat par des tests des
  deux côtés
- montée en compétence Rust nécessaire pour l'équipe
