---
title: "Plateforme"
description: "Comment fonctionne la plateforme obop : des instances avec une base de données par entreprise, les modules et la place de marché, les groupes de permissions, les classes de données et le masquage, les approbations, et les interfaces MCP, REST et CLI."
lead: "Un seul programme, un seul modèle de données, trois points d'entrée. Cette page explique comment les pièces s'assemblent."
eyebrow: "Comment ça marche"
---

## Une instance par entreprise

Chaque entreprise fonctionne dans sa propre **instance**, avec sa propre base de données, ses propres utilisateurs et sa propre adresse. C'est vous qui exécutez le programme : un `obop serve` est une instance. Le même programme a un second mode, un plan de contrôle (`obop platform serve`), où chacun s'inscrit et crée ses propres instances ; la personne qui crée une instance en est le premier administrateur. SpaceCorps n'héberge pas encore obop pour d'autres personnes. Voir [Instances](/docs/concepts/instances/).

Le stockage est SQLite pour essayer, PostgreSQL pour un usage réel. Pour l'instant, les enregistrements sont stockés sous forme de documents JSON ; la couche de commandes vérifie les types de champs, et un gestionnaire de schéma est prévu.

## Les modules sont des applications

Une application est un **module** : une description de modèles, de champs, de groupes de permissions et d'actions de flux de travail. Six applications intégrées ont une page sur ce site (Contacts, Products, CRM, Sales, Invoicing, Connectors). 24 autres sont fournies sous forme de paquets, et 22 d'entre elles ont aussi une page. Vous ajoutez des applications depuis la place de marché en un clic, ou vous importez votre propre fichier.

Comme chaque module passe par un seul registre, un contact est le même contact dans CRM, Sales et Invoicing, et chaque module reçoit la même API, les mêmes écrans, les mêmes permissions et les mêmes règles de confidentialité. Voir [Applications et modules](/docs/concepts/apps-and-modules/) et la [liste des applications](/apps/).

## Place de marché

La place de marché est un hôte statique qui sert `index.json` et un fichier `obop.toml` par paquet. Le catalogue fourni est livré avec le programme ; vous pouvez aussi pointer une instance vers votre propre hôte. Chaque téléchargement est validé comme un import. Voir [Place de marché](/marketplace/).

## Permissions

Les groupes de permissions tels que `crm.user` ou `invoicing.manager` sont référencés par les modèles, les champs et les actions. Les personnes détiennent des groupes ; les jetons d'agent détiennent un sous-ensemble des groupes de leur propriétaire. Voir [Groupes de permissions](/docs/concepts/permission-groups/).

## Classes de données et masquage

Chaque champ déclare une classe de données (aucune, annuaire public, interne, RH confidentielle, catégorie particulière, financière, secrète) et s'il s'agit d'une donnée personnelle. Les personnes voient les valeurs selon leurs permissions. Les agents voient les données personnelles sous forme de jetons de substitution stables, et les classes sensibles pas du tout ; ils peuvent renvoyer des jetons pour désigner une valeur sans la voir. Voir [Classes de données et masquage](/docs/concepts/data-classes-and-masking/).

## Flux de travail et approbations

Les changements d'état sont des actions nommées. Une action peut exiger un groupe plus strict et peut requérir une approbation. Un agent ne peut que proposer une action qui requiert une approbation : le serveur renvoie un plan et ne change rien tant qu'une personne ne l'a pas approuvé. La suppression d'enregistrements est également seulement proposée par les agents. Voir [Flux de travail et approbations](/docs/concepts/workflows-and-approvals/).

## Trois interfaces, un seul jeu de commandes

| Interface       | Pour                      | Comment                                                                                                   |
| --------------- | ------------------------- | --------------------------------------------------------------------------------------------------------- |
| Application web | Les personnes             | Générée à partir des descriptions de modules : listes, tableaux, formulaires.                             |
| REST            | Scripts, services, agents | `POST /api/{model}/{command}` avec un jeton porteur. [Référence REST](/docs/reference/rest-api/)          |
| MCP             | Agents IA                 | `obop mcp` via stdio ou `/mcp` via HTTP, un outil par commande. [Se connecter via MCP](/docs/agents/mcp/) |
| CLI             | Shells et cron            | `obop call model command key=value`. [Référence de la CLI](/docs/reference/cli/)                          |

Toutes appellent le même répartiteur : les contrôles de permissions, le masquage et les approbations sont donc identiques.

## Ce qu'elle n'est pas (encore)

Sachez-le avant de vous engager :

- Il n'y a pas de niveaux de code au-delà des paquets déclaratifs (ni composants WebAssembly ni applications distantes), donc un paquet d'application n'exécute pas de code propre et ne peut pas déclarer d'appels vers d'autres systèmes, mais il peut déclarer des totaux calculés et des actions qui modifient d'autres enregistrements de la même base de données.
- Seules certaines applications (par exemple Expenses, Timesheets, les demandes de congé de HR, Documents et Dashboards) limitent les enregistrements qu'une personne voit. CRM, Sales et Invoicing ne le font pas.
- Le journal de sécurité enregistre par exemple les événements de compte, de connexion, de paramètres de sécurité, de modules et de signature, mais ce n'est pas un historique des modifications des enregistrements de l'entreprise.
- La tâche d'export prévue pour les données de toute une entreprise n'est pas réalisée. Une liste s'exporte en CSV depuis l'application web (jusqu'à 20 000 enregistrements), et `obop backup --plain` écrit les enregistrements et les fichiers téléversés d'une base de données dans un fichier non chiffré (les valeurs de courte durée, comme les sessions, en sont exclues ; les secrets restent scellés, mais si vous n'avez pas défini `OBOP_SECRET_KEY`, la clé qu'obop a générée pour eux se trouve dans le fichier).
- Un plan de contrôle n'active pas les sauvegardes planifiées des instances qu'il crée (si vous faites tourner une instance vous-même, vous pouvez les activer).

La [feuille de route](/roadmap/) indique ce qui est réalisé et ce qui ne l'est pas.
