---
title: "Conectar mediante MCP"
description: "Conecta Claude Code, Cursor, VS Code, Windsurf, Codex o Gemini CLI a una instancia de obop por URL con el servidor MCP remoto (Streamable HTTP, bearer token), o ejecuta obop mcp sobre stdio. Configuración exacta para cada cliente."
lead: "Cada instancia de obop tiene un servidor MCP en /mcp. Apunta un cliente a la URL con un token de agente y cada comando que el token puede usar se convierte en una herramienta MCP: no hace falta ningún binario local."
---

## MCP remoto: conectar por URL

Una instancia sirve el [Model Context Protocol](https://modelcontextprotocol.io/) mediante **Streamable HTTP** en `https://YOUR-INSTANCE/mcp`. Lo tienen tanto una instancia alojada (por ejemplo en Sliplane) como un `obop serve` local; no hay nada que activar.

- **Autenticación:** `Authorization: Bearer <token>` en cada petición, con un [token de agente](/docs/agents/tokens/) (o un token de sesión obtenido al iniciar sesión). El token se comprueba exactamente igual que en REST, de modo que la lista de herramientas, los permisos, el enmascarado y las aprobaciones son los que el token tiene mediante [REST](/docs/agents/rest/).
- **Todavía no hay OAuth.** obop no tiene un servidor de autorización OAuth. El endpoint responde `401` con `WWW-Authenticate: Bearer resource_metadata="https://YOUR-INSTANCE/.well-known/oauth-protected-resource"`, como pide la especificación de autorización de MCP, y ese documento indica claramente que los tokens se obtienen con `POST /api/auth/login` y `POST /api/auth/agent-token` y no nombra ningún servidor de autorización. Los clientes que solo pueden conectarse mediante un flujo OAuth en el navegador (por ejemplo el diálogo de "conector personalizado" de las aplicaciones de Claude) **todavía no pueden conectarse**; usa uno de los clientes siguientes, que aceptan una cabecera.
- **Revisiones del protocolo:** `2025-11-25`, `2025-06-18` y `2025-03-26`, negociadas en `initialize`. La revisión más reciente, `2026-07-28`, no tiene estado y funciona de otra manera; obop todavía no la implementa. Un cliente que admite ambas vuelve a `initialize` cuando recibe un `400` simple, que es lo que envía obop ante una cabecera de versión no admitida.
- **Sesiones:** `initialize` devuelve un `Mcp-Session-Id`; envíalo en todas las peticiones posteriores. La sesión conserva la bóveda de enmascarado, de modo que un token de máscara como `NAME_1` significa lo mismo en todas las llamadas de la sesión y puedes enviarlo de vuelta. Una sesión distinta inicia una bóveda nueva. `DELETE /mcp` termina una sesión; una sesión desconocida o caducada responde `404` y el cliente vuelve a empezar con `initialize`. Una sesión pertenece al token que la abrió y caduca tras dos horas sin ninguna petición.
- **Las respuestas son JSON simple.** No hay un flujo SSE del servidor al cliente (`GET /mcp` responde `405`), y los lotes JSON-RPC solo se aceptan en `2025-03-26`, la única revisión que los tiene.
- **Límites:** cuerpos de petición de hasta 1 MiB (`413`), 600 peticiones por minuto y token (`429` con `Retry-After`, ajustable con `OBOP_MCP_RATE_PER_MINUTE`), 25 sesiones abiertas por token.
- **Navegadores:** una petición con cabecera `Origin` se rechaza con `403` salvo que el origen sea la URL pública de la propia instancia (`OBOP_PUBLIC_URL`), el mismo origen en un host de loopback, o esté incluido en `OBOP_ALLOWED_ORIGINS` (separado por comas). Esto protege las instancias locales frente al DNS rebinding. Los clientes que no son navegadores no envían `Origin`. Detrás de un proxy, define `OBOP_PUBLIC_URL` (por ejemplo `https://acme.example.com`) para que los documentos de descubrimiento indiquen la dirección pública.

### Herramientas

`tools/list` muestra una herramienta por cada comando que el token puede usar, con el nombre `<model>__<command>`: `party__search`, `lead__create`, `invoice__post`. Cada una tiene un `inputSchema` en JSON Schema generado a partir del modelo: los campos de dinero y numéricos son enteros, las fechas son `format: date`, los campos de selección son un `enum` con sus opciones, un campo de referencia es una cadena que nombra su modelo de destino, y los campos obligatorios están en `required`. `list` y `search` aceptan `filter`, `order`, `limit` y `offset`; `get`, `delete` y las acciones aceptan un `id`. Las anotaciones marcan las lecturas (`readOnlyHint`) y el borrado (`destructiveHint`). Una herramienta adicional de solo lectura, `obop__meta`, devuelve lo mismo que `GET /api/meta`: empieza por ahí. Una herramienta que el token no puede usar no aparece en la lista y responde como una herramienta que no existe.

Los resultados tienen `structuredContent` y el mismo JSON como texto:

```json
{ "status": "ok", "value": [{ "id": "...", "name": "NAME_1" }], "denied": ["party.tax_id"] }
```

- Un paso que necesita a una persona (todo borrado, contabilizar una factura) no se ejecuta para un agente. El resultado es un resultado normal con `"status": "approval_required"`, un `plan_id` y una instrucción para consultar a una persona, que lo aprueba en el back office o con `POST /api/approve/<plan_id>`. Consulta [El ciclo de aprobación](/docs/agents/approvals/).
- Una entrada rechazada (un valor incorrecto, un campo que falta) es un error de herramienta: `isError: true` y el mensaje del servidor en `structuredContent.error`.
- `initialize` también devuelve `instructions`, un texto breve que indica al modelo cómo usar la instancia.

`tools/list` se pagina con un `cursor` solo cuando un token ve más de 200 herramientas. La lista no anuncia cambios (`listChanged` es `false`): tras instalar un módulo, inicia una sesión nueva.

### Compruébalo con curl

```bash
export OBOP_URL=https://YOUR-INSTANCE
export OBOP_TOKEN=obop_...
curl -si -X POST "$OBOP_URL/mcp" \
  -H "Authorization: Bearer $OBOP_TOKEN" \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

Copia la cabecera `mcp-session-id` de la respuesta, y después envía `{"jsonrpc":"2.0","method":"notifications/initialized"}` (respuesta `202`) y `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`, ambos con `-H "Mcp-Session-Id: <id>"`.

### Configuración de clientes remotos

Sustituye `https://YOUR-INSTANCE/mcp` y guarda el token en una variable de entorno o en un almacén de secretos. Los formatos siguientes se tomaron de la documentación de cada cliente el 2026-10-17. Solo ejecutamos obop contra el cliente de línea de comandos MCP Inspector (`vp dlx @modelcontextprotocol/inspector --cli URL --transport http --header "Authorization: Bearer ..." --method tools/list`) y contra nuestras propias pruebas de Playwright. No iniciamos los demás clientes, así que cada fragmento está **documentado, no probado**.

#### Claude Code

```bash
claude mcp add --transport http obop https://YOUR-INSTANCE/mcp \
  --header "Authorization: Bearer $OBOP_TOKEN"
```

Añade `--scope project` para escribir un `.mcp.json` compartido o `--scope user` para usarlo en todos los proyectos. En un archivo compartido, lee el token del entorno para que no se suba al repositorio:

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

Si el servidor rechaza la cabecera, Claude Code informa de un fallo de conexión en lugar de iniciar OAuth. Compruébalo con `/mcp`.

#### Claude Desktop y claude.ai

Las aplicaciones añaden servidores remotos como _conectores personalizados_ (Settings, Connectors, Add custom connector) y los autentican con OAuth. Allí no se ofrece una cabecera bearer estática, y obop no tiene servidor OAuth, por lo que **una instancia alojada de obop todavía no se puede añadir como conector**. La configuración stdio [de más abajo](#claude-desktop-stdio) funciona en Claude Desktop con un binario local. Un puente como el paquete comunitario `mcp-remote` puede añadir una cabecera a un servidor remoto para los clientes que solo hablan stdio; no lo hemos probado con obop.

#### Cursor

`.cursor/mcp.json` (proyecto) o `~/.cursor/mcp.json` (todos los proyectos):

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

`.vscode/mcp.json`. La clave de nivel superior es `servers`, y una entrada pide el token para que no se escriba en disco:

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

La documentación de VS Code no mostraba un ejemplo completo con cabeceras y una entrada, así que este fragmento se ha montado a partir de sus tablas de campos.

#### Windsurf

La documentación de Windsurf ahora está en el sitio de Devin Desktop. Indica `~/.config/devin/mcp_config.json` (macOS y Linux) y `%APPDATA%\devin\mcp_config.json` (Windows); las instalaciones antiguas de Windsurf usaban `~/.codeium/windsurf/mcp_config.json`. Comprueba cuál lee tu versión. Los servidores remotos usan `serverUrl` (o `url`) y `headers`, y `${env:NAME}` se expande:

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

En `~/.codex/config.toml`. `bearer_token_env_var` nombra la variable de entorno que contiene el token; Codex lo envía como bearer token en `Authorization`:

```toml
[mcp_servers.obop]
url = "https://YOUR-INSTANCE/mcp"
bearer_token_env_var = "OBOP_TOKEN"
```

`codex mcp add` documenta `--url` solo con opciones de OAuth, así que usa el archivo para un bearer token.

#### Gemini CLI

La documentación que leímos indica que Gemini CLI fue sustituido por Antigravity CLI el 2026-06-18; consulta la página de la herramienta que uses. El formato de Gemini CLI es `httpUrl` para Streamable HTTP (`url` es para el transporte SSE anterior), en `~/.gemini/settings.json`:

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

o `gemini mcp add --transport http --header "Authorization: Bearer $OBOP_TOKEN" obop https://YOUR-INSTANCE/mcp`. No pudimos confirmar que este archivo expanda variables de entorno en `headers`, por lo que aquí el token está escrito en claro.

### Descubrimiento

Una instancia publica lo siguiente sin necesidad de token:

| URL                                     | Qué                                                                                                                                                                  |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/.well-known/oauth-protected-resource` | Metadatos del recurso protegido OAuth (RFC 9728) para `/mcp`. Indica que no hay servidor de autorización. También en `/.well-known/oauth-protected-resource/mcp`.    |
| `/.well-known/mcp/server-card.json`     | Tarjeta del servidor MCP: nombre, versión, el endpoint `streamable-http`, versiones del protocolo, la cabecera `Authorization`. También en `/mcp/server-card`.       |
| `/.well-known/ai-catalog.json`          | Un AI Catalog de una sola entrada que apunta a la tarjeta del servidor.                                                                                              |
| `/api/openapi.json`, `/api/meta`        | El documento OpenAPI tiene un campo `x-mcp` con el endpoint, el transporte y las versiones del protocolo; `GET /api/meta` lleva una cabecera `Link` con `rel="mcp"`. |

La tarjeta del servidor es una extensión de la especificación MCP que aún se está desarrollando (SEP-2127, mantenida en un repositorio de incubación), por lo que su forma puede cambiar; el archivo lo indica en su `$comment`. También incluye los campos simples `serverInfo`, `transport`, `endpoint` y `capabilities` de un borrador anterior, para los analizadores que los buscan.

## MCP sobre stdio (binario local)

Úsalo cuando el cliente solo pueda iniciar un programa local, o cuando ejecutes obop en la misma máquina. El servidor remoto descrito arriba no necesita nada de esto.

### Cómo funciona

`obop mcp` es un servidor MCP que habla JSON-RPC por la entrada y la salida estándar. Un cliente MCP lo inicia como proceso hijo. Este servidor:

- abre él mismo la base de datos de la instancia, por lo que debe ejecutarse en una máquina que pueda llegar a la base de datos (archivo SQLite o URL de PostgreSQL);
- identifica al agente mediante un token de agente;
- lista las mismas herramientas, con los mismos esquemas, que el servidor remoto (ambos transportes usan un único camino de código): una herramienta por cada comando que ese token puede usar, con el nombre `<model>__<command>`. El doble guion bajo ocupa el lugar de la barra en la ruta REST;
- responde a `tools/call` a través del mismo despachador que REST, de modo que se aplican el enmascarado y las aprobaciones. Los tokens de máscara son estables mientras el proceso siga en ejecución.

### Antes de empezar

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
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

El servidor responde con la revisión que pide el cliente si es una de `2025-11-25`, `2025-06-18` y `2025-03-26`, y admite `initialize`, `ping`, `tools/list` y `tools/call`. Los argumentos de una herramienta son el mismo JSON que el [cuerpo REST](/docs/reference/rest-api/).

Los fragmentos siguientes usan `/usr/local/bin/obop` y un token de ejemplo. Sustituye ambos. Los formatos se comprobaron con la documentación de cada cliente el 2026-10-16; lo que no se pudo confirmar está indicado.

### Claude Desktop (stdio)

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

### Claude Code (stdio)

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

### Cursor (stdio)

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

### VS Code (stdio)

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

### Windsurf (stdio)

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

### Codex CLI (stdio)

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

### Gemini CLI (stdio)

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

### Cualquier otro cliente MCP

obop es un servidor stdio sencillo. Sea cual sea el aspecto del archivo del cliente, necesita un `command` (`obop`), unos `args` (`["mcp"]`) y dos variables de entorno. Si el cliente solo admite servidores remotos, usa el servidor remoto descrito arriba.

## Qué ve el agente

Tras conectar, pide al agente que liste sus herramientas. Deberías ver solo los comandos que permiten sus scopes, con una nota en los que requieren aprobación. Prueba primero una lectura, por ejemplo _"Lista las tres oportunidades más recientes"_, y después consulta [El ciclo de aprobación](/docs/agents/approvals/).

Si el cliente no muestra herramientas o da un error, ve a [Solución de problemas](/docs/agents/troubleshooting/).

## Verificado y no verificado

| Elemento                                                                                                 | Estado                                                                                            |
| -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Handshake de `obop mcp`, `tools/list`, `tools/call`, resultado de aprobación                             | Ejecutado contra una instancia local real.                                                        |
| `/mcp` remoto: handshake, herramientas, enmascarado, ciclo de aprobación, sesiones, 401, origen, límites | Pruebas automatizadas (Rust sobre HTTP, Playwright) contra una instancia real.                    |
| `/mcp` remoto con un cliente MCP real                                                                    | Cliente de línea de comandos MCP Inspector 2.11.0: `initialize` y `tools/list`, `tools/call`.     |
| Suite oficial de conformidad de MCP                                                                      | No es significativa: necesita un servidor sin autenticación y sus propias herramientas de prueba. |
| Configuración remota de Claude Code, Cursor, VS Code, Windsurf, Codex CLI, Gemini CLI                    | Tomada de la documentación de cada cliente; los clientes no se iniciaron.                         |
| Formatos de configuración stdio                                                                          | Tomados de la documentación de cada cliente. No iniciamos todos los clientes.                     |
| Ubicación del archivo de Windsurf                                                                        | Sin confirmar, véase arriba.                                                                      |
| Conectores de Claude Desktop y claude.ai                                                                 | Necesitan OAuth; obop todavía no se puede añadir allí.                                            |
