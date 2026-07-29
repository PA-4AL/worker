# ADR-0004 — Une politique de retry par criticité de tâche

- **Date** : 2026-07-15
- **Statut** : accepté
- **Portée** : worker

## Contexte

Le brief confie au worker la responsabilité de la politique de reprise, et
demande d'en déterminer la criticité **pour chaque tâche**. Or les deux tâches du
worker n'ont pas du tout le même profil d'échec.

## Décision

Une politique par type de tâche (`RetryPolicy::for_task`), avec backoff
exponentiel :

| Tâche | Tentatives | Raison |
|---|---|---|
| `import_excel` | **3**, délai initial 1 s | l'échec peut être transitoire (fichier volumineux, mémoire, aléa d'exécution) |
| `export_excel` | **1** | génération purement locale et déterministe : un échec se reproduirait à l'identique, rejouer ne servirait à rien |

Au-delà, la réponse d'échec est publiée avec son code, son message et le **nombre
de tentatives**, pour que le backend puisse l'afficher.

## Conséquences

- on ne rejoue que ce qui a une chance d'aboutir : pas de temps perdu ni de
  charge inutile
- l'utilisateur voit un échec définitif avec un message exploitable, plutôt
  qu'une tâche qui reste éternellement « en cours »
- second filet indépendant : l'abonnement Pub/Sub redéposera un message non
  acquitté après 5 tentatives dans la file de rebut. Les deux niveaux se
  complètent — le retry applicatif traite l'échec de **traitement**, Pub/Sub
  l'échec de **livraison**
