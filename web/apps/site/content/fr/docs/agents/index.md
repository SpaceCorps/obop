---
title: "Agents"
description: "Comment configurer des agents IA pour travailler avec obop : le modèle d'agent, les jetons à périmètre limité, les données personnelles masquées, et les approbations qui laissent une personne aux commandes des étapes marquées comme soumises à approbation."
lead: "obop traite les agents IA comme des utilisateurs à part entière, volontairement limités. Cette page explique le modèle ; les suivantes montrent comment en configurer un."
---

## Le modèle d'agent

Un **agent** est tout ce qui agit sur obop avec un jeton d'agent : un client MCP tel que Claude Desktop ou Cursor, un script qui appelle l'API REST, ou la ligne de commande. Les agents travaillent sur les mêmes enregistrements et avec les mêmes commandes que les personnes. Quatre règles rendent cela sûr.

1. **Périmètre limité.** Un jeton d'agent porte une liste de groupes de permissions (ses _scopes_). Il ne peut que restreindre ce que son propriétaire est autorisé à faire, jamais l'élargir. Demander un groupe que le propriétaire ne détient pas est refusé.
2. **Masqué.** Les données personnelles reviennent sous forme de jetons de substitution stables, comme `EMAIL_1`. Les champs financiers, RH et de catégorie particulière ne sont pas communiqués. Un agent peut tout de même raisonner sur les enregistrements et renvoyer des jetons, mais il ne voit jamais les valeurs réelles. Voir [Jetons de masquage](/docs/agents/masking/).
3. **Proposer, ne pas valider.** Un agent peut exécuter les actions ordinaires dans ses scopes, mais pour les commandes marquées comme soumises à approbation (par exemple valider une facture, rembourser, publier et supprimer un enregistrement), il ne peut que _proposer_. Le serveur renvoie un identifiant de plan et ne modifie rien. Une personne approuve. Voir [La boucle d'approbation](/docs/agents/approvals/).
4. **Une seule porte.** REST, MCP et la CLI passent par un même répartiteur : un agent a donc exactement les mêmes permissions, le même masquage et les mêmes approbations, quel que soit son mode de connexion.

## Trois façons de se connecter

| Interface | À utiliser quand                                                                                                       | Page                                      |
| --------- | ---------------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| MCP       | Un client MCP (Claude Desktop, Claude Code, Cursor, VS Code, Windsurf et d'autres) doit appeler obop comme des outils. | [Se connecter via MCP](/docs/agents/mcp/) |
| REST      | Un script ou un service appelle des points de terminaison HTTP.                                                        | [Utiliser l'API REST](/docs/agents/rest/) |
| CLI       | Un shell ou une tâche cron appelle `obop call`.                                                                        | [Utiliser la CLI](/docs/agents/cli/)      |

## Mise en place en cinq étapes

1. Décidez de ce que l'agent doit faire, et des groupes que cela exige. Moins, c'est mieux.
2. [Créez un jeton d'agent](/docs/agents/tokens/) avec ces seuls scopes.
3. Connectez l'agent via [MCP](/docs/agents/mcp/), [REST](/docs/agents/rest/) ou la [CLI](/docs/agents/cli/).
4. Essayez une lecture sans risque, puis une modification qui requiert une approbation, et approuvez-la en tant que personne.
5. Lisez [Bonnes pratiques de sécurité](/docs/agents/safety/) avant de confier le jeton à quoi que ce soit qui fonctionne sans surveillance.

## Ce qui existe aujourd'hui, et ce qui n'existe pas

- Réalisé : jetons à périmètre limité, masquage et restauration, plans d'approbation, MCP via stdio et via HTTP à l'adresse `/mcp`, REST et la CLI.
- Réalisé, mais essayé seulement avec notre propre client de test et des tests automatisés, pas avec Claude Desktop, claude.ai ni Claude Code : la connexion par le navigateur (OAuth) pour les clients MCP. Pas encore réalisé : un écran pour créer ou révoquer des jetons d'agent ; une liste des approbations en attente dans l'interface. Les jetons d'agent n'expirent pas et il n'existe pas encore de commande pour en révoquer un seul ; une application connectée par la connexion du navigateur peut être révoquée sous _Personnaliser, Sécurité, Applications connectées_. Ces points sont rappelés là où ils comptent.

Consultez [Exemples de flux de travail](/docs/agents/workflows/) pour des requêtes et des enchaînements de départ, et [Dépannage](/docs/agents/troubleshooting/) quand quelque chose ne fonctionne pas.
