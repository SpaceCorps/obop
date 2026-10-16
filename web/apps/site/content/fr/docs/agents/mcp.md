---
title: "Se connecter via MCP"
description: "Connectez Claude Desktop, Claude Code, Cursor, VS Code, Windsurf, Codex ou Gemini CLI à obop avec le Model Context Protocol : configuration exacte pour chaque client, avec obop mcp via stdio."
lead: "obop fournit un serveur MCP. Chaque commande que le jeton peut utiliser devient un outil MCP. Cette page donne la configuration des clients courants."
---

## Fonctionnement

`obop mcp` est un serveur MCP qui communique en JSON-RPC sur l'entrée et la sortie standard. Un client MCP le démarre comme processus enfant. Il :

- ouvre lui-même la base de données de l'instance : il doit donc s'exécuter sur une machine qui peut atteindre la base (fichier SQLite ou URL PostgreSQL) ;
- identifie l'agent grâce à un jeton d'agent ;
- liste un outil par commande que ce jeton peut utiliser, nommé `<model>__<command>` : `lead__search`, `lead__create`, `invoice__post`. Le double tiret bas remplace la barre oblique du chemin REST ;
- répond à `tools/call` par le même répartiteur que REST : le masquage et les approbations s'appliquent donc.

> **Instances hébergées.** Il n'existe pas encore de transport MCP distant (HTTP), et `obop mcp` a besoin d'accéder à la base de données. Vous ne pouvez donc pas, aujourd'hui, pointer un client MCP de bureau vers une instance hébergée. Pour les instances hébergées, utilisez [REST](/docs/agents/rest/). MCP via HTTP avec OAuth est prévu.

## Avant de commencer

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
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Le serveur annonce la version de protocole `2025-06-18` et prend en charge `initialize`, `ping`, `tools/list` et `tools/call`. Les schémas d'entrée des outils ne sont aujourd'hui que `{"type":"object"}` : un client ne peut donc pas valider les arguments ; ceux-ci sont le même JSON que le [corps REST](/docs/reference/rest-api/).

Les extraits ci-dessous utilisent `/usr/local/bin/obop` et un jeton fictif. Remplacez les deux. Les formats ont été vérifiés dans la documentation de chaque client le 2026-10-16 ; ce qui n'a pas pu être confirmé est signalé.

## Claude Desktop

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

## Claude Code

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

## Cursor

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

## VS Code

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

## Windsurf

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

## Codex CLI

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

## Gemini CLI

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

## Tout autre client MCP

obop est un simple serveur stdio. Quelle que soit la forme du fichier du client, il faut une `command` (`obop`), des `args` (`["mcp"]`) et deux variables d'environnement. Si le client ne prend en charge que les serveurs distants, il ne peut pas encore se connecter.

## Ce que voit l'agent

Après la connexion, demandez à l'agent de lister ses outils. Vous ne devriez voir que les commandes que ses scopes autorisent, avec le suffixe « (agents propose; a human approves) » sur celles qui requièrent une approbation. Essayez d'abord une lecture, par exemple _« Liste les trois pistes les plus récentes »_, puis consultez [La boucle d'approbation](/docs/agents/approvals/).

Si le client n'affiche aucun outil ou une erreur, rendez-vous sur [Dépannage](/docs/agents/troubleshooting/).

## Vérifié et non vérifié

| Élément                                                                                     | État                                                                                   |
| ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Poignée de main `obop mcp`, `tools/list`, `tools/call`, résultat d'approbation              | Exécutés sur une vraie instance locale.                                                |
| Formats de configuration de Claude Desktop, Claude Code, Cursor, VS Code, Codex, Gemini CLI | Tirés de la documentation de chaque client. Nous n'avons pas démarré tous les clients. |
| Emplacement du fichier de Windsurf                                                          | Non confirmé, voir ci-dessus.                                                          |
