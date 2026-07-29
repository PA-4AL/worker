# ADR-0006 — Consommer la file en pull

- **Date** : 2026-07-28
- **Statut** : accepté
- **Portée** : worker, infra

## Contexte

Pub/Sub propose deux modes de livraison : *pull* (le consommateur va chercher les
messages) et *push* (le service envoie une requête HTTP au consommateur). Les
traitements Excel peuvent durer, et le worker maîtrise sa propre politique de
reprise.

## Décision

Le worker consomme en **pull** (`subscription.pull(10)`), acquitte chaque message
après traitement, et publie sa réponse sur le topic de sortie.

Le sens inverse — worker vers backend — utilise au contraire le **push** : le
backend n'a ainsi rien à interroger, et le jeton OIDC de la livraison lui prouve
l'identité de l'appelant.

## Conséquences

- le worker contrôle son rythme : pas de traitement imposé pendant qu'il est
  occupé, et son retry applicatif s'applique avant tout acquittement
- **conséquence de coût** : un consommateur pull doit rester allumé, d'où
  `min-instances = 1` et une CPU allouée en permanence sur Cloud Run — environ
  8 $/mois. À `0`, il ne consommerait jamais la file
- migrer le worker vers un endpoint push permettrait le scale-to-zero : c'est la
  piste identifiée si le coût devenait un problème
