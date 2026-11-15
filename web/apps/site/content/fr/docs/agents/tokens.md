---
title: "Créer un jeton d'agent"
description: "Créez un jeton d'agent obop à périmètre limité dans le back-office, via REST (POST /api/auth/agent-token) ou avec la CLI (obop agent-token). Le jeton n'est affiché qu'une fois et ne peut que restreindre ce que son propriétaire a le droit de faire."
lead: "Un jeton d'agent est un jeton porteur qui appartient à une personne et porte une liste de groupes de permissions. Créez-en un par agent et par tâche."
---

## Ce qu'est un jeton

- Il appartient à la personne qui l'a créé (son _propriétaire_) et agit avec un sous-ensemble des groupes de cette personne.
- Il n'est affiché qu'**une seule fois**. obop n'en conserve qu'une empreinte : un jeton perdu ne peut pas être récupéré, il faut en créer un nouveau.
- Il commence par `obop_`. Traitez-le comme un mot de passe.
- Seule une personne peut créer des jetons. Un agent ne peut pas en émettre un autre.
- Il n'expire pas. Son propriétaire peut le révoquer à tout moment (voir [Lister et révoquer](#lister-et-revoquer)). Il cesse aussi de fonctionner tant que le compte de son propriétaire est désactivé, et définitivement lorsque le propriétaire est mis hors service (offboarding).

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

## Dans le back-office

Ouvrez _Personnaliser, Sécurité_ et allez à **Jetons d'agent**. Saisissez un libellé (le programme ou l'agent qui utilisera le jeton), cochez les groupes, puis choisissez **Créer un jeton d'agent**. Le jeton n'est affiché qu'une fois, avec un bouton **Copier** ; choisissez **Terminé** quand vous l'avez conservé. La section liste aussi les jetons que vous avez créés, chacun avec **Révoquer**.

## Via REST

Connectez-vous en tant que personne, puis demandez un jeton. Cela suppose une instance en cours d'exécution ; les exemples utilisent une instance locale sur le port 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

La réponse est `{"token":"obop_..."}` : un jeton de session valable 12 heures. Si la connexion à deux facteurs est activée pour le compte, la réponse est à la place `{"mfa_required":true,"mfa_token":"..."}` ; envoyez `{"mfa_token":"...","code":"123456"}`, avec un code de votre application d'authentification, à `POST /api/auth/login/mfa` pour obtenir le jeton de session. Utilisez le jeton de session comme jeton porteur pour créer le jeton d'agent :

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

`scopes` est obligatoire et ne doit pas être vide. Il nomme des groupes inscrits sur votre propre compte. Un administrateur (une personne qui détient `platform.admin`) peut nommer n'importe quel groupe de l'instance, car ce groupe les autorise déjà tous. Le jeton ne porte toutefois que les groupes que vous nommez : `*` n'est pas un groupe et est refusé, et un groupe qu'aucun module installé ne déclare est refusé lui aussi. `label` fait au plus 80 caractères, sur une seule ligne. Sur une instance créée par un plan de contrôle, utilisez son adresse à la place de `127.0.0.1:8080`, et connectez-vous avec votre adresse e-mail d'inscription comme nom d'utilisateur.

## Avec la CLI

Si vous exécutez obop vous-même, vous pouvez émettre un jeton directement en ligne de commande. Elle s'adresse directement à la base de données : lancez-la donc là où la base est accessible :

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Ajoutez `--db postgres://...` (ou définissez `OBOP_DATABASE_URL`) si l'instance n'utilise pas la valeur par défaut `sqlite://obop.db`. Les scopes suivent la même règle que via REST, pour l'utilisateur que vous nommez. La ligne de commande ne permet pas encore de lister ni de révoquer des jetons.

## Lister et révoquer

Vous voyez et révoquez vos propres jetons d'agent sous _Personnaliser, Sécurité, Jetons d'agent_, ou via REST avec votre jeton de session :

```bash
curl -s http://127.0.0.1:8080/api/auth/agent-tokens \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

```json
{
  "tokens": [
    {
      "id": "5f0c...",
      "label": "triage-bot",
      "scopes": ["base.user", "crm.user"],
      "created_at": 1794650000
    }
  ],
  "delegable": ["base.user", "crm.user"]
}
```

`tokens` ne contient jamais de jeton, seulement le nom de chacun et ce qu'il peut faire. `delegable` est la liste des groupes que vous pouvez mettre sur un nouveau jeton. Pour révoquer un jeton, envoyez un POST à son `id` :

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-tokens/5f0c.../revoke \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

Le jeton cesse de fonctionner immédiatement. Seul le propriétaire peut le faire : un administrateur ne peut pas lister ni révoquer un à un les jetons d'une autre personne (désactiver le compte ou le mettre hors service met fin à tous). La création et la révocation d'un jeton sont toutes deux inscrites dans le journal de sécurité, sans le jeton.

## Utiliser le jeton pour MCP

Le même jeton connecte un client MCP au serveur MCP distant de l'instance, `https://YOUR-INSTANCE/mcp` : envoyez-le sous la forme `Authorization: Bearer obop_...` (les clients ont un champ `headers` ou une variable de jeton porteur pour cela). Un jeton créé avec la CLI ou via REST fonctionne pour les deux ; il n'y a rien de spécifique à MCP à créer. Gardez ces points à l'esprit :

- Créez un jeton distinct par client et ne lui donnez que les scopes dont ce client a besoin. La liste des outils MCP correspond exactement aux commandes que les scopes autorisent.
- Un jeton de session obtenu à la connexion fonctionne aussi, mais il expire après 12 heures (un jeton d'agent n'expire pas), et il agit en votre nom : les données personnelles ne sont pas masquées et les commandes soumises à approbation s'exécutent sans approbation. Utilisez un jeton d'agent pour un agent.
- Le jeton doit être envoyé à chaque requête, pas seulement à la première ; les clients le font dès qu'il figure dans leur configuration.
- Les clients qui prennent en charge une connexion par navigateur (OAuth) n'ont pas besoin d'un jeton collé : ils se connectent par une connexion et reçoivent un jeton valable une heure qui ne fonctionne que sur `/mcp`. Nous ne l'avons pas encore essayé avec Claude Desktop, claude.ai ou Claude Code ; voir [Se connecter via MCP](/docs/agents/mcp/).

## Le conserver en lieu sûr

- Placez-le dans une variable d'environnement ou un coffre de secrets, pas dans un dépôt.
- Privilégiez un jeton par agent et par tâche, chacun avec seulement les scopes dont cet agent a besoin.
- Les jetons placés dans le fichier de configuration d'un client sont lisibles par tout ce qui peut lire ce fichier.

## Erreurs

| Réponse                                                                            | Signification                                                                                                          |
| ---------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                          | `scopes` était vide.                                                                                                   |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}`     | Vous avez demandé un groupe qui ne figure pas sur votre compte, et vous n'êtes pas administrateur.                     |
| `400 {"error":"nope.user is not a registered permission group, ..."}`              | Un administrateur a demandé un groupe qu'aucun module installé ne déclare.                                             |
| `400 {"error":"only a human user can issue agent tokens"}`                         | Vous avez utilisé un jeton d'agent pour en demander un autre.                                                          |
| `401`                                                                              | Jeton de session manquant, incorrect ou expiré.                                                                        |
| `403 {"error":"two-factor authentication must be set up before you can continue"}` | Votre compte doit d'abord configurer la connexion à deux facteurs ; d'ici là, cette session ne peut que la configurer. |

Ensuite : [connectez-vous via MCP](/docs/agents/mcp/) ou [utilisez l'API REST](/docs/agents/rest/).
