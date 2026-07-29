# ADR-0003 — Transporter les fichiers en base64 dans le message

- **Date** : 2026-07-15
- **Statut** : accepté
- **Portée** : worker, backend

## Contexte

Le worker doit recevoir des fichiers `.xlsx` à analyser et en renvoyer d'autres
après génération. Ne pouvant accéder à aucun stockage partagé du backend
(ADR-0002), le contenu doit voyager d'une manière ou d'une autre.

## Décision

Encoder le fichier en **base64 dans le corps du message** Pub/Sub, à l'aller
(`file_base64` de la demande) comme au retour (`file_base64` de la réponse).

Écarté : un **bucket Cloud Storage** avec transmission d'URL — plus élégant à
grande échelle, mais cela ajoute un service à provisionner, des droits à accorder
au worker, et un cycle de vie d'objets à gérer, pour des fichiers de quelques
centaines de kilooctets.

## Conséquences

- aucune infrastructure supplémentaire, aucun droit de stockage pour le worker
- le message reste autoportant : rejouable tel quel depuis la file de rebut
- **limite dure de 10 Mo par message Pub/Sub**, vérifiée côté backend qui refuse
  au-delà (`413`, soit ~6,7 Mo de fichier réel après encodage)
- si des fichiers plus gros devenaient nécessaires, cet ADR serait remplacé : la
  colonne `jobs.file_url` existe déjà en base pour accueillir ce cas
