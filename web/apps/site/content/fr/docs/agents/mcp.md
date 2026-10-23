---
title: "Se connecter via MCP"
description: "Connectez Claude Code, Cursor, VS Code, Windsurf, Codex ou Gemini CLI à une instance obop par URL avec le serveur MCP distant (Streamable HTTP, jeton porteur), ou lancez obop mcp via stdio. Configuration exacte pour chaque client."
lead: "Chaque instance obop dispose d'un serveur MCP à l'adresse /mcp. Pointez un client vers l'URL avec un jeton d'agent : chaque commande que le jeton peut utiliser devient un outil MCP, sans binaire local."
---

## MCP distant : connexion par URL

Une instance sert le [Model Context Protocol](https://modelcontextprotocol.io/) en **Streamable HTTP** à l'adresse `https://YOUR-INSTANCE/mcp`. Une instance hébergée (par exemple sur Sliplane) comme un `obop serve` local l'offrent ; il n'y a rien à activer.

- **Authentification :** une connexion dans le navigateur (OAuth, voir ci-dessous) pour Claude Desktop, claude.ai et Claude Code, ou `Authorization: Bearer <token>` sur chaque requête avec un jeton d'agent. Dans les deux cas le jeton est vérifié comme pour REST. Un jeton absent ou invalide reçoit `401` avec `WWW-Authenticate: Bearer resource_metadata="https://YOUR-INSTANCE/.well-known/oauth-protected-resource/mcp"`, comme l'exige la spécification d'autorisation MCP ; ce document nomme le serveur d'autorisation de l'instance (`/.well-known/oauth-authorization-server`). Un jeton émis par la connexion ne vaut que pour `/mcp`, pas pour `/api`.
- **Révisions du protocole :** `2025-11-25`, `2025-06-18` et `2025-03-26`, négociées dans `initialize`. La révision la plus récente, `2026-07-28`, est sans état et fonctionne différemment ; obop ne l'implémente pas encore. Un client qui parle les deux revient à `initialize` lorsqu'il reçoit un simple `400`, ce que obop envoie pour un en-tête de version non pris en charge.
- **Sessions :** `initialize` renvoie un `Mcp-Session-Id` ; envoyez-le dans chaque requête suivante. La session conserve le coffre de masquage : un jeton de masque tel que `NAME_1` a donc le même sens dans tous les appels de la session et vous pouvez le renvoyer. Une autre session démarre un nouveau coffre. `DELETE /mcp` termine une session ; une session inconnue ou expirée répond `404`, et le client repart avec `initialize`. Une session appartient au jeton qui l'a ouverte et expire après deux heures sans requête.
- **Les réponses sont du JSON simple.** Il n'y a pas de flux SSE du serveur vers le client (`GET /mcp` répond `405`), et les lots JSON-RPC ne sont acceptés que sur `2025-03-26`, la seule révision qui les prévoit.
- **Limites :** corps de requête jusqu'à 1 MiB (`413`), 600 requêtes par minute et par jeton (`429` avec `Retry-After`, réglable avec `OBOP_MCP_RATE_PER_MINUTE`), 25 sessions ouvertes par jeton.
- **Navigateurs :** une requête avec un en-tête `Origin` est refusée avec `403`, sauf si l'origine est l'URL publique de l'instance (`OBOP_PUBLIC_URL`), la même origine sur un hôte de bouclage, ou figure dans `OBOP_ALLOWED_ORIGINS` (séparées par des virgules). Cela protège les instances locales contre le DNS rebinding. Les clients qui ne sont pas des navigateurs n'envoient pas d'`Origin`. Derrière un proxy, définissez `OBOP_PUBLIC_URL` (par exemple `https://acme.example.com`) pour que les documents de découverte indiquent l'adresse publique.

### Se connecter avec le navigateur (OAuth)

Claude Desktop, claude.ai et Claude Code peuvent se connecter à une instance sans jeton collé. Dans l'application, ajoutez un connecteur personnalisé avec l'adresse `https://YOUR-INSTANCE/mcp` (Claude Code : `claude mcp add --transport http obop https://YOUR-INSTANCE/mcp`, puis `/mcp` et choisir de s'authentifier). L'application trouve seule la connexion et ouvre dans le navigateur une page de **votre instance**. Vous y :

1. vous connectez avec votre compte obop, et saisissez votre code à deux facteurs si le compte en a un (une clé d'accès fonctionne aussi, comme seconde étape ou seule si vous l'avez autorisé dans Sécurité) ;
2. voyez quelle application demande l'accès et vers quelle adresse le navigateur reviendra ;
3. cochez les groupes de permissions à transmettre, choisissez la durée de l'accès (1, 7, 30 ou 90 jours) et appuyez sur **Autoriser**.

Ce que cela signifie :

- **L'application ne peut que vous restreindre.** Les groupes proposés sont ceux que vous détenez ; l'application agit en votre nom avec ceux que vous cochez, jamais avec plus (les [jetons d'agent](/docs/agents/tokens/) fonctionnent de la même façon). Ses actions sont enregistrées comme celles de votre agent.
- **Les approbations exigent toujours une personne.** Une étape qui demande une approbation attend un humain dans obop ; l'application ne peut pas approuver ses propres propositions.
- **Jetons de courte durée.** L'application reçoit un jeton d'accès valable une heure et un jeton de renouvellement utilisé une seule fois puis remplacé. Si un jeton de renouvellement est utilisé deux fois, toute la connexion prend fin. Un jeton d'accès issu de cette connexion ne vaut que pour `/mcp`.
- **Vous gardez la main.** _Personnaliser, Sécurité, Applications connectées_ liste chaque connexion avec ses groupes et sa date de fin ; **Révoquer** y met fin aussitôt. La connexion, le refus, la révocation et un jeton réutilisé figurent dans le journal de sécurité.
- **Aucune confiance d'avance.** N'importe quelle application peut s'enregistrer ; la page affiche donc le nom choisi et l'adresse de retour, et dit qu'obop ne peut pas vérifier qui l'exploite. Les adresses de retour doivent être en `https` (ou `http` sur `localhost`, `127.0.0.1` ou `[::1]`), les enregistrements sont limités et ceux qui ne servent pas expirent.

Protocole, pour les auteurs de clients : métadonnées RFC 9728 et RFC 8414, enregistrement dynamique des clients (RFC 7591, clients publics), code d'autorisation avec PKCE **S256** (obligatoire), paramètre `resource` (RFC 8707, qui doit être l'adresse `/mcp` de l'instance), rotation des jetons de renouvellement avec détection de réutilisation, révocation (RFC 7009) et paramètre `iss` dans la redirection (RFC 9207). Points d'accès : `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`. Une redirection loopback (`http://127.0.0.1:PORT/...`) peut utiliser n'importe quel port (RFC 8252) ; toute autre doit correspondre exactement. Une instance derrière un proxy a besoin de `OBOP_PUBLIC_URL` pour que les adresses de ces documents soient les publiques. Nous l'avons construit d'après la spécification et testé avec notre propre client dans des tests automatisés ; nous n'avons **pas encore connecté les vraies applications Claude**.

### Outils

`tools/list` affiche un outil par commande que le jeton peut utiliser, nommé `<model>__<command>` : `party__search`, `lead__create`, `invoice__post`. Chacun possède un `inputSchema` JSON Schema généré à partir du modèle : les champs monétaires et numériques sont des entiers, les dates ont `format: date`, les sélections sont un `enum` de leurs options, un champ de référence est une chaîne qui nomme son modèle cible, et les champs obligatoires figurent dans `required`. `list` et `search` acceptent `filter`, `order`, `limit` et `offset` ; `get`, `delete` et les actions acceptent un `id`. Des annotations signalent les lectures (`readOnlyHint`) et la suppression (`destructiveHint`). Un outil supplémentaire en lecture seule, `obop__meta`, renvoie ce que renvoie `GET /api/meta` : commencez par lui. Un outil que le jeton ne peut pas utiliser n'est pas listé et répond comme un outil qui n'existe pas.

Les résultats contiennent `structuredContent` et le même JSON en texte :

```json
{ "status": "ok", "value": [{ "id": "...", "name": "NAME_1" }], "denied": ["party.tax_id"] }
```

- Une étape qui requiert une personne (toute suppression, la validation d'une facture) ne s'exécute pas pour un agent. Le résultat est un résultat normal avec `"status": "approval_required"`, un `plan_id` et une instruction de demander à une personne, qui l'approuve dans le back-office ou avec `POST /api/approve/<plan_id>`. Voir [La boucle d'approbation](/docs/agents/approvals/).
- Une entrée refusée (valeur incorrecte, champ manquant) est une erreur d'outil : `isError: true` et le message du serveur dans `structuredContent.error`.
- `initialize` renvoie aussi `instructions`, un court texte qui indique au modèle comment utiliser l'instance.

`tools/list` n'est paginé avec un `cursor` que lorsqu'un jeton voit plus de 200 outils. La liste n'annonce pas les changements (`listChanged` vaut `false`) : après l'installation d'un module, démarrez une nouvelle session.

### Vérifier avec curl

```bash
export OBOP_URL=https://YOUR-INSTANCE
export OBOP_TOKEN=obop_...
curl -si -X POST "$OBOP_URL/mcp" \
  -H "Authorization: Bearer $OBOP_TOKEN" \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

Copiez l'en-tête `mcp-session-id` de la réponse, puis envoyez `{"jsonrpc":"2.0","method":"notifications/initialized"}` (réponse `202`), puis `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`, les deux avec `-H "Mcp-Session-Id: <id>"`.

### Configuration des clients distants

Remplacez `https://YOUR-INSTANCE/mcp` et conservez le jeton dans une variable d'environnement ou un coffre de secrets. Les formats ci-dessous proviennent de la documentation de chaque client au 2026-10-17. Nous n'avons essayé obop qu'avec le client en ligne de commande MCP Inspector (`vp dlx @modelcontextprotocol/inspector --cli URL --transport http --header "Authorization: Bearer ..." --method tools/list`) et avec nos propres tests Playwright. Nous n'avons pas démarré les autres clients : chaque extrait est donc **documenté, non essayé**.

#### Claude Code

```bash
claude mcp add --transport http obop https://YOUR-INSTANCE/mcp \
  --header "Authorization: Bearer $OBOP_TOKEN"
```

Ajoutez `--scope project` pour écrire un fichier `.mcp.json` partagé, ou `--scope user` pour l'utiliser dans tous les projets. Dans un fichier partagé, lisez le jeton dans l'environnement pour qu'il ne soit pas versionné :

```json
{
  "mcpServers": {
    "obop": {
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${OBOP_TOKEN}" }
    }
  }
}
```

Si le serveur rejette l'en-tête, Claude Code signale un échec de connexion au lieu de lancer OAuth. Vérifiez avec `/mcp`.

#### Claude Desktop et claude.ai

Les applications ajoutent les serveurs distants comme _connecteurs personnalisés_ (Settings, Connectors, Add custom connector) et les authentifient avec OAuth. Saisissez `https://YOUR-INSTANCE/mcp` et suivez la connexion dans le navigateur décrite ci-dessus ; aucun jeton n'est collé. Pour claude.ai, l'instance doit être joignable depuis Internet en `https`, car ce sont les serveurs d'Anthropic qui font les requêtes. La configuration stdio [ci-dessous](#claude-desktop-stdio) fonctionne dans Claude Desktop avec un binaire local. Un pont comme le paquet communautaire `mcp-remote` peut ajouter un en-tête à un serveur distant pour les clients qui ne parlent que stdio ; nous ne l'avons pas testé avec obop.

#### Cursor

`.cursor/mcp.json` (projet) ou `~/.cursor/mcp.json` (tous les projets) :

```json
{
  "mcpServers": {
    "obop": {
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### VS Code

`.vscode/mcp.json`. La clé de premier niveau est `servers`, et une entrée demande le jeton à la saisie pour qu'il ne soit pas écrit sur le disque :

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "obop-token",
      "description": "obop agent token",
      "password": true
    }
  ],
  "servers": {
    "obop": {
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${input:obop-token}" }
    }
  }
}
```

La documentation de VS Code ne montrait pas d'exemple complet avec des en-têtes et une entrée ; cet extrait est donc assemblé à partir de ses tableaux de champs.

#### Windsurf

La documentation de Windsurf se trouve désormais sur le site de Devin Desktop. Elle indique `~/.config/devin/mcp_config.json` (macOS et Linux) et `%APPDATA%\devin\mcp_config.json` (Windows) ; les anciennes installations de Windsurf utilisaient `~/.codeium/windsurf/mcp_config.json`. Vérifiez lequel votre version lit. Les serveurs distants utilisent `serverUrl` (ou `url`) et `headers`, et `${env:NAME}` est développé :

```json
{
  "mcpServers": {
    "obop": {
      "serverUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### Codex CLI

Dans `~/.codex/config.toml`. `bearer_token_env_var` nomme la variable d'environnement qui contient le jeton ; Codex l'envoie comme jeton porteur `Authorization` :

```toml
[mcp_servers.obop]
url = "https://YOUR-INSTANCE/mcp"
bearer_token_env_var = "OBOP_TOKEN"
```

`codex mcp add` ne documente `--url` qu'avec des options OAuth ; utilisez donc le fichier pour un jeton porteur.

#### Gemini CLI

La documentation que nous avons lue indique que Gemini CLI a été remplacé par Antigravity CLI le 2026-06-18 ; consultez la page de l'outil que vous utilisez. Le format de Gemini CLI est `httpUrl` pour Streamable HTTP (`url` correspond à l'ancien transport SSE), dans `~/.gemini/settings.json` :

```json
{
  "mcpServers": {
    "obop": {
      "httpUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer your-agent-token" }
    }
  }
}
```

ou `gemini mcp add --transport http --header "Authorization: Bearer $OBOP_TOKEN" obop https://YOUR-INSTANCE/mcp`. Nous n'avons pas pu confirmer que ce fichier développe les variables d'environnement dans `headers` ; le jeton est donc écrit en clair ici.

### Découverte

Une instance publie ces ressources sans jeton :

| URL                                       | Contenu                                                                                                                                                                               |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/.well-known/oauth-protected-resource`   | Métadonnées de ressource protégée OAuth (RFC 9728) pour `/mcp`, nommant le serveur d'autorisation. Aussi à `/.well-known/oauth-protected-resource/mcp`, que la réponse `401` indique. |
| `/.well-known/oauth-authorization-server` | Métadonnées du serveur d'autorisation OAuth (RFC 8414) : les points d'accès, PKCE `S256` uniquement, clients publics uniquement.                                                      |
| `/.well-known/mcp/server-card.json`       | Fiche du serveur MCP : nom, version, point d'accès `streamable-http`, versions du protocole, en-tête `Authorization`. Aussi à `/mcp/server-card`.                                     |
| `/.well-known/ai-catalog.json`            | Un catalogue d'IA à une seule entrée qui pointe vers la fiche du serveur.                                                                                                             |
| `/api/openapi.json`, `/api/meta`          | Le document OpenAPI a un champ `x-mcp` avec le point d'accès, le transport et les versions du protocole ; `GET /api/meta` porte un en-tête `Link` avec `rel="mcp"`.                   |

La fiche du serveur est une extension de la spécification MCP encore en cours d'élaboration (SEP-2127, conservée dans un dépôt d'incubation) ; sa forme peut donc changer, ce que le fichier indique dans son `$comment`. Elle contient aussi les champs simples `serverInfo`, `transport`, `endpoint` et `capabilities` d'une version préliminaire antérieure, pour les outils d'analyse qui les recherchent.

## MCP via stdio (binaire local)

Utilisez cette méthode lorsque le client ne peut que démarrer un programme local, ou lorsque vous exécutez obop sur la même machine. Le serveur distant ci-dessus n'en a pas besoin.

### Fonctionnement

`obop mcp` est un serveur MCP qui communique en JSON-RPC sur l'entrée et la sortie standard. Un client MCP le démarre comme processus enfant. Il :

- ouvre lui-même la base de données de l'instance : il doit donc s'exécuter sur une machine qui peut atteindre la base (fichier SQLite ou URL PostgreSQL) ;
- identifie l'agent grâce à un jeton d'agent ;
- liste les mêmes outils, avec les mêmes schémas, que le serveur distant (les deux transports passent par un seul chemin de code) : un outil par commande que ce jeton peut utiliser, nommé `<model>__<command>`. Le double tiret bas remplace la barre oblique du chemin REST ;
- répond à `tools/call` par le même répartiteur que REST : le masquage et les approbations s'appliquent donc. Les jetons de masque sont stables aussi longtemps que le processus s'exécute.

### Avant de commencer

1. Une base de données d'instance en fonctionnement, et le binaire `obop` sur la machine qui exécute le client MCP (ou son chemin absolu).
2. Un [jeton d'agent](/docs/agents/tokens/) avec les scopes voulus.
3. L'adresse de la base de données, sous la forme `OBOP_DATABASE_URL` ou `--db`. La valeur par défaut est `sqlite://obop.db` dans le répertoire courant ; indiquez donc un chemin absolu : `sqlite:///home/me/obop.db`.

La commande que chaque client exécute est :

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

Elle lit `OBOP_TOKEN` et `OBOP_DATABASE_URL` dans l'environnement si les options sont absentes. Préférez l'environnement : le jeton n'apparaît ainsi pas dans la liste des processus.

Vérifiez d'abord à la main. Cette commande envoie une requête `initialize` et une requête `tools/list`, puis affiche les réponses :

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Le serveur répond avec la révision demandée par le client si c'est l'une de `2025-11-25`, `2025-06-18` et `2025-03-26`, et prend en charge `initialize`, `ping`, `tools/list` et `tools/call`. Les arguments d'un outil sont le même JSON que le [corps REST](/docs/reference/rest-api/).

Les extraits ci-dessous utilisent `/usr/local/bin/obop` et un jeton fictif. Remplacez les deux. Les formats ont été vérifiés dans la documentation de chaque client le 2026-10-16 ; ce qui n'a pas pu être confirmé est signalé.

### Claude Desktop (stdio)

Ouvrez _Settings_, puis _Developer_, puis _Edit Config_. Le fichier est `~/Library/Application Support/Claude/claude_desktop_config.json` sous macOS et `%APPDATA%\Claude\claude_desktop_config.json` sous Windows. Ajoutez :

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "obop_your_agent_token",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Quittez puis relancez Claude Desktop. Claude demande votre accord avant d'utiliser un outil.

### Claude Code (stdio)

Utilisez la ligne de commande. Tout ce qui suit `--` est la commande du serveur :

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Placez au moins une autre option (ici `--transport stdio`) entre le dernier `--env` et le nom du serveur. Ajoutez `--scope project` pour écrire plutôt un fichier `.mcp.json` partagé, ou `--scope user` pour l'utiliser dans tous les projets. Un fichier de projet ressemble à ceci ; la forme `${...}` lit la valeur dans votre environnement, de sorte que le jeton n'est pas versionné :

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

Vérifiez avec `claude mcp get obop` ou `/mcp` dans Claude Code.

### Cursor (stdio)

Créez `.cursor/mcp.json` dans un projet, ou `~/.cursor/mcp.json` pour tous les projets :

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Les exemples de Cursor omettent `type` pour les serveurs stdio ; son tableau de champs le mentionne. `${env:NAME}` lit la valeur dans l'environnement dans lequel Cursor a été lancé.

### VS Code (stdio)

Créez `.vscode/mcp.json`. La clé de premier niveau de VS Code est `servers`, et non `mcpServers`, et il peut demander le jeton à la saisie pour qu'il ne soit jamais écrit sur le disque :

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "obop-token",
      "description": "obop agent token",
      "password": true
    }
  ],
  "servers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${input:obop-token}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

La commande guidée _MCP: Add Server_ de VS Code peut plutôt proposer un fichier `.mcp.json` portable au format `mcpServers` ; selon sa documentation, les deux sont acceptés.

### Windsurf (stdio)

La documentation de Windsurf a été déplacée vers le site de Devin Desktop et décrit le fichier comme `mcp_config.json`. Nous n'avons pas pu confirmer l'emplacement actuel du fichier pour Windsurf lui-même. Windsurf utilisait historiquement `~/.codeium/windsurf/mcp_config.json` ; la page de Devin Desktop indique `~/.config/devin/mcp_config.json` (macOS et Linux) et `%APPDATA%\devin\mcp_config.json` (Windows). Vérifiez l'emplacement dans les paramètres de votre version. Le format est le même, de forme `mcpServers` :

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

### Codex CLI (stdio)

Ajoutez une table à `~/.codex/config.toml` (ou à `.codex/config.toml` dans un projet de confiance), ou exécutez `codex mcp add` :

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` transmet `OBOP_TOKEN` depuis votre shell au lieu de l'écrire dans le fichier.

### Gemini CLI (stdio)

Ajoutez à `~/.gemini/settings.json` :

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "$OBOP_TOKEN",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

La documentation de Gemini CLI indique qu'il a été remplacé par un autre outil en 2026 ; consultez cette page pour votre version. Nous n'avons pas essayé obop avec lui.

### Tout autre client MCP

obop est un simple serveur stdio. Quelle que soit la forme du fichier du client, il faut une `command` (`obop`), des `args` (`["mcp"]`) et deux variables d'environnement. Si le client ne prend en charge que les serveurs distants, utilisez le serveur distant ci-dessus.

## Ce que voit l'agent

Après la connexion, demandez à l'agent de lister ses outils. Vous ne devriez voir que les commandes que ses scopes autorisent, avec une note sur celles qui requièrent une approbation. Essayez d'abord une lecture, par exemple _« Liste les trois pistes les plus récentes »_, puis consultez [La boucle d'approbation](/docs/agents/approvals/).

Si le client n'affiche aucun outil ou une erreur, rendez-vous sur [Dépannage](/docs/agents/troubleshooting/).

Chaque modèle que l'agent peut créer a aussi un outil `<model>__bulk_create` : jusqu'à 1 000 lignes en un appel (`rows` ou texte `csv`), tout ou rien par défaut, `mode: "partial"` pour garder les bonnes lignes et lister les mauvaises, `dry_run: true` pour vérifier d'abord, et une `idempotency_key` pour rendre une nouvelle tentative sûre. Masquage, portée et approbation sont ceux de `create`.

Un agent dont le propriétaire est administrateur voit aussi les outils `user_account` (`user_account__list`, `__get`, `__add`, `__set_groups`, `__offboard`, `__disable`, `__enable`). Ceux qui modifient un compte renvoient toujours `approval_required` : un administrateur approuve le plan, qui s'exécute avec le compte actuel de cet administrateur. `__add` exige un mot de passe choisi par une personne ; le serveur ne remet jamais un mot de passe généré à un agent.

## Vérifié et non vérifié

| Élément                                                                                                              | État                                                                                           |
| -------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Poignée de main `obop mcp`, `tools/list`, `tools/call`, résultat d'approbation                                       | Exécutés sur une vraie instance locale.                                                        |
| `/mcp` distant : poignée de main, outils, masquage, boucle d'approbation, sessions, 401, contrôle d'origine, limites | Tests automatisés (Rust via HTTP, Playwright) sur une vraie instance.                          |
| `/mcp` distant avec un vrai client MCP                                                                               | Client en ligne de commande MCP Inspector 2.11.0 : `initialize` et `tools/list`, `tools/call`. |
| Suite de conformité MCP officielle                                                                                   | Non pertinente : elle exige un serveur sans authentification et ses propres outils de test.    |
| Configuration distante de Claude Code, Cursor, VS Code, Windsurf, Codex CLI, Gemini CLI                              | Tirée de la documentation de chaque client ; les clients n'ont pas été démarrés.               |
| Formats de configuration stdio                                                                                       | Tirés de la documentation de chaque client. Nous n'avons pas démarré tous les clients.         |
| Emplacement du fichier de Windsurf                                                                                   | Non confirmé, voir ci-dessus.                                                                  |
| Connexion dans le navigateur (OAuth) : enregistrement, PKCE, consentement, jetons, réutilisation, révocation         | Tests automatisés (Rust, et Playwright dans un vrai navigateur) avec notre propre client.      |
| Connecteurs de Claude Desktop et de claude.ai, connexion de Claude Code                                              | **Non essayés** avec les vraies applications ; construits d'après la spécification.            |
