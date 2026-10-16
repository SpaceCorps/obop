---
title: "Conectar mediante MCP"
description: "Conecta Claude Desktop, Claude Code, Cursor, VS Code, Windsurf, Codex o Gemini CLI a obop con el Model Context Protocol: configuración exacta para cada cliente, usando obop mcp sobre stdio."
lead: "obop incluye un servidor MCP. Cada comando que el token puede usar se convierte en una herramienta MCP. Esta página da la configuración para los clientes más habituales."
---

## Cómo funciona

`obop mcp` es un servidor MCP que habla JSON-RPC por la entrada y la salida estándar. Un cliente MCP lo inicia como proceso hijo. Este servidor:

- abre él mismo la base de datos de la instancia, por lo que debe ejecutarse en una máquina que pueda llegar a la base de datos (archivo SQLite o URL de PostgreSQL);
- identifica al agente mediante un token de agente;
- lista una herramienta por cada comando que ese token puede usar, con el nombre `<model>__<command>`: `lead__search`, `lead__create`, `invoice__post`. El doble guion bajo ocupa el lugar de la barra en la ruta REST;
- responde a `tools/call` a través del mismo despachador que REST, de modo que se aplican el enmascarado y las aprobaciones.

> **Instancias alojadas.** Todavía no hay un transporte MCP remoto (HTTP), y `obop mcp` necesita la base de datos. Hoy no puedes apuntar un cliente MCP de escritorio a una instancia alojada. Para las instancias alojadas usa [REST](/docs/agents/rest/). MCP sobre HTTP con OAuth está previsto.

## Antes de empezar

1. La base de datos de una instancia en marcha y el binario `obop` en la máquina que ejecuta el cliente MCP (o usa su ruta absoluta).
2. Un [token de agente](/docs/agents/tokens/) con los scopes que quieras.
3. La dirección de la base de datos, como `OBOP_DATABASE_URL` o `--db`. El valor por defecto es `sqlite://obop.db` en el directorio actual, así que indica una ruta absoluta: `sqlite:///home/me/obop.db`.

El comando que ejecuta cada cliente es:

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

Si faltan las opciones, lee `OBOP_TOKEN` y `OBOP_DATABASE_URL` del entorno. Es preferible usar el entorno: así el token no aparece en la lista de procesos.

Compruébalo primero a mano. Esto envía una petición `initialize` y otra `tools/list` e imprime las respuestas:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

El servidor informa de la versión de protocolo `2025-06-18` y admite `initialize`, `ping`, `tools/list` y `tools/call`. Hoy los esquemas de entrada de las herramientas son solo `{"type":"object"}`, por lo que un cliente no puede validar los argumentos; los argumentos son el mismo JSON que el [cuerpo REST](/docs/reference/rest-api/).

Los fragmentos siguientes usan `/usr/local/bin/obop` y un token de ejemplo. Sustituye ambos. Los formatos se comprobaron con la documentación de cada cliente el 2026-10-16; lo que no se pudo confirmar está indicado.

## Claude Desktop

Abre _Settings_, luego _Developer_ y después _Edit Config_. El archivo es `~/Library/Application Support/Claude/claude_desktop_config.json` en macOS y `%APPDATA%\Claude\claude_desktop_config.json` en Windows. Añade:

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

Cierra y reinicia Claude Desktop. Claude pide tu aprobación antes de usar una herramienta.

## Claude Code

Usa la línea de comandos. Todo lo que va después de `--` es el comando del servidor:

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Coloca al menos otra opción (aquí `--transport stdio`) entre el último `--env` y el nombre del servidor. Añade `--scope project` para escribir en su lugar un `.mcp.json` compartido, o `--scope user` para usarlo en todos los proyectos. Un archivo de proyecto tiene este aspecto; la forma `${...}` lee el valor de tu entorno para que el token no se suba al repositorio:

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

Compruébalo con `claude mcp get obop` o con `/mcp` dentro de Claude Code.

## Cursor

Crea `.cursor/mcp.json` en un proyecto, o `~/.cursor/mcp.json` para todos los proyectos:

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

Los ejemplos propios de Cursor omiten `type` en los servidores stdio; su tabla de campos sí lo incluye. `${env:NAME}` lee del entorno en el que se inició Cursor.

## VS Code

Crea `.vscode/mcp.json`. La clave de nivel superior de VS Code es `servers`, no `mcpServers`, y puede pedir el token para que nunca se escriba en disco:

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

El comando guiado _MCP: Add Server_ de VS Code puede sugerir en su lugar un `.mcp.json` portable con el formato `mcpServers`; según su documentación, se aceptan ambos.

## Windsurf

La documentación de Windsurf se ha trasladado al sitio de Devin Desktop y describe el archivo como `mcp_config.json`. No pudimos confirmar la ubicación actual del archivo para el propio Windsurf. Históricamente, Windsurf usaba `~/.codeium/windsurf/mcp_config.json`; la página de Devin Desktop indica `~/.config/devin/mcp_config.json` (macOS y Linux) y `%APPDATA%\devin\mcp_config.json` (Windows). Comprueba la ubicación en los ajustes de tu versión. El formato es la misma estructura `mcpServers`:

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

Añade una tabla a `~/.codex/config.toml` (o a `.codex/config.toml` en un proyecto de confianza), o ejecuta `codex mcp add`:

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` reenvía `OBOP_TOKEN` desde tu shell en lugar de escribirlo en el archivo.

## Gemini CLI

Añade a `~/.gemini/settings.json`:

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

La documentación de Gemini CLI indica que en 2026 fue sustituido por otra herramienta, así que consulta esa página para tu versión. No hemos probado obop con él.

## Cualquier otro cliente MCP

obop es un servidor stdio sencillo. Sea cual sea el aspecto del archivo del cliente, necesita un `command` (`obop`), unos `args` (`["mcp"]`) y dos variables de entorno. Si el cliente solo admite servidores remotos, todavía no puede conectarse.

## Qué ve el agente

Tras conectar, pide al agente que liste sus herramientas. Deberías ver solo los comandos que permiten sus scopes, con el sufijo "(agents propose; a human approves)" en los que requieren aprobación. Prueba primero una lectura, por ejemplo _"Lista las tres oportunidades más recientes"_, y después consulta [El ciclo de aprobación](/docs/agents/approvals/).

Si el cliente no muestra herramientas o da un error, ve a [Solución de problemas](/docs/agents/troubleshooting/).

## Verificado y no verificado

| Elemento                                                                                     | Estado                                                                        |
| -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Handshake de `obop mcp`, `tools/list`, `tools/call`, resultado de aprobación                 | Ejecutado contra una instancia local real.                                    |
| Formatos de configuración de Claude Desktop, Claude Code, Cursor, VS Code, Codex, Gemini CLI | Tomados de la documentación de cada cliente. No iniciamos todos los clientes. |
| Ubicación del archivo de Windsurf                                                            | Sin confirmar, véase arriba.                                                  |
