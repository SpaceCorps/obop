---
title: "Créer un jeton d'agent"
description: "Créez un jeton d'agent obop à périmètre limité via REST (POST /api/auth/agent-token) ou avec la CLI (obop agent-token). Le jeton n'est affiché qu'une fois et ne peut que restreindre les groupes de permissions de son propriétaire."
lead: "Un jeton d'agent est un jeton porteur qui appartient à une personne et porte une liste de groupes de permissions. Créez-en un par agent et par tâche."
---

## Ce qu'est un jeton

- Il appartient à la personne qui l'a créé (son _propriétaire_) et agit avec un sous-ensemble des groupes de cette personne.
- Il n'est affiché qu'**une seule fois**. obop n'en conserve qu'une empreinte : un jeton perdu ne peut pas être récupéré, il faut en créer un nouveau.
- Il commence par `obop_`. Traitez-le comme un mot de passe.
- Seule une personne peut créer des jetons. Un agent ne peut pas en émettre un autre.
- Il n'expire pas, et cesse de fonctionner lorsque son propriétaire est supprimé. Il n'existe pas encore de commande pour révoquer un jeton en particulier (voir [Sécurité](/docs/agents/safety/)).

## Choisir les scopes

Les scopes sont des [groupes de permissions](/docs/concepts/permission-groups/). Regardez ce que l'agent doit faire et ne lui donnez que ces groupes. Groupes courants des applications intégrées :

| Groupe              | Permet à l'agent de                                                     |
| ------------------- | ----------------------------------------------------------------------- |
| `base.user`         | lire et écrire les Contacts (les données personnelles restent masquées) |
| `crm.user`          | lire et écrire les pistes et les faire avancer dans le pipeline         |
| `sales.user`        | lire et écrire les devis et les commandes                               |
| `invoicing.user`    | lire et écrire les factures et annuler les brouillons                   |
| `invoicing.manager` | proposer la validation d'une facture (une personne approuve encore)     |
| `product.user`      | lire les produits et les prix                                           |

Les paquets installés ajoutent leurs propres groupes, par exemple `helpdesk.agent`. Les [pages des applications](/apps/) indiquent les groupes de chacune.

## Via REST

Connectez-vous en tant que personne, puis demandez un jeton. Cela suppose une instance en cours d'exécution ; les exemples utilisent une instance locale sur le port 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

La réponse est `{"token":"obop_..."}` : un jeton de session valable 12 heures. Utilisez-le comme jeton porteur pour créer le jeton d'agent :

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

`scopes` est obligatoire et doit nommer au moins un groupe que vous détenez. `label` est un texte libre. Sur une instance hébergée, utilisez son adresse à la place de `127.0.0.1:8080`, et connectez-vous avec votre adresse e-mail d'inscription comme nom d'utilisateur.

## Avec la CLI

Si vous exécutez obop vous-même, vous pouvez émettre un jeton directement en ligne de commande. Elle s'adresse directement à la base de données : lancez-la donc là où la base est accessible :

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Ajoutez `--db postgres://...` (ou définissez `OBOP_DATABASE_URL`) si l'instance n'utilise pas la valeur par défaut `sqlite://obop.db`.

## Le conserver en lieu sûr

- Placez-le dans une variable d'environnement ou un coffre de secrets, pas dans un dépôt.
- Privilégiez un jeton par agent et par tâche, pour pouvoir les distinguer et en supprimer un sans toucher aux autres.
- Les jetons placés dans le fichier de configuration d'un client sont lisibles par tout ce qui peut lire ce fichier.

## Erreurs

| Réponse                                                                        | Signification                                                 |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                      | `scopes` était vide.                                          |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}` | Vous avez demandé un groupe que vous ne détenez pas.          |
| `400 {"error":"only a human user can issue agent tokens"}`                     | Vous avez utilisé un jeton d'agent pour en demander un autre. |
| `401`                                                                          | Jeton de session manquant, incorrect ou expiré.               |

Ensuite : [connectez-vous via MCP](/docs/agents/mcp/) ou [utilisez l'API REST](/docs/agents/rest/).
