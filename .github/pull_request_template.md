## Quoi

<!-- Ce que fait cette PR, en une ou deux phrases. -->

## Pourquoi

<!-- Le besoin ou le bug à l'origine du changement. -->

## Comment vérifier

<!-- Étapes de test manuel, ou les tests automatisés ajoutés. -->

## Checklist

- [ ] La CI est verte (lint + tests)
- [ ] Un seul sujet dans cette PR
- [ ] Aucun secret, `.env`, clé ou état Terraform ajouté
- [ ] Documentation mise à jour si le comportement ou le déploiement change
- [ ] Ordre inter-repos respecté si la fonctionnalité en traverse plusieurs
      (`infra → backend → worker → frontend`, cf. infra/docs/GIT-FLOW.md)
