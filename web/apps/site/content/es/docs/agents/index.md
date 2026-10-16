---
title: "Agentes"
description: "Cómo configurar agentes de IA para que trabajen con obop: el modelo de agente, tokens con alcance limitado, datos personales enmascarados y aprobaciones que dejan a una persona al mando de todo lo irreversible."
lead: "obop trata a los agentes de IA como usuarios de primera clase, limitados a propósito. Esta página explica el modelo; las siguientes muestran cómo configurar uno."
---

## El modelo de agente

Un **agente** es cualquier cosa que actúa sobre obop con un token de agente: un cliente MCP como Claude Desktop o Cursor, un script que llama a la API REST, o la línea de comandos. Los agentes trabajan sobre los mismos registros y con los mismos comandos que las personas. Cuatro reglas lo hacen seguro.

1. **Alcance limitado.** Un token de agente lleva una lista de grupos de permisos (sus _scopes_). Solo puede reducir lo que su propietario puede hacer, nunca ampliarlo. Si pide un grupo que el propietario no tiene, se rechaza.
2. **Enmascarado.** Los datos personales se devuelven como tokens marcadores estables, como `EMAIL_1`. Los campos financieros, de RR. HH. y de categorías especiales se retienen. Un agente puede seguir razonando sobre los registros y devolver tokens, pero nunca ve los valores reales. Consulta [Tokens de enmascarado](/docs/agents/masking/).
3. **Proponer, no ejecutar.** Un agente puede ejecutar acciones ordinarias dentro de sus scopes, pero para todo lo irreversible (contabilizar una factura, pagar, reembolsar, enviar, publicar, eliminar) solo puede _proponerlo_. El servidor devuelve el identificador de un plan y no cambia nada. Una persona aprueba. Consulta [El ciclo de aprobación](/docs/agents/approvals/).
4. **La misma puerta.** REST, MCP y la CLI pasan por un único despachador, así que un agente tiene exactamente los mismos permisos, el mismo enmascarado y las mismas aprobaciones se conecte como se conecte.

## Tres formas de conectar

| Superficie      | Úsala cuando                                                                                                          | Página                                     |
| --------------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| MCP sobre stdio | Un cliente MCP (Claude Desktop, Claude Code, Cursor, VS Code, Windsurf y otros) debe llamar a obop como herramientas. | [Conectar mediante MCP](/docs/agents/mcp/) |
| REST            | Un script o servicio llama a endpoints HTTP.                                                                          | [Usar la API REST](/docs/agents/rest/)     |
| CLI             | Una shell o una tarea cron llama a `obop call`.                                                                       | [Usar la CLI](/docs/agents/cli/)           |

## Configuración en cinco pasos

1. Decide qué debe hacer el agente y qué grupos necesita para ello. Cuanto menos, mejor.
2. [Crea un token de agente](/docs/agents/tokens/) con solo esos scopes.
3. Conecta el agente mediante [MCP](/docs/agents/mcp/), [REST](/docs/agents/rest/) o la [CLI](/docs/agents/cli/).
4. Prueba una lectura inofensiva y después un cambio que requiera aprobación, y apruébalo como persona.
5. Lee las [Buenas prácticas de seguridad](/docs/agents/safety/) antes de entregar el token a cualquier proceso desatendido.

## Qué existe hoy y qué no

- Construido: tokens con alcance limitado, enmascarado y restauración, planes de aprobación, MCP sobre stdio, REST y la CLI.
- Aún no construido: un transporte MCP HTTP (remoto) con OAuth, por lo que todavía no se puede llegar a las instancias alojadas mediante MCP; una pantalla para crear o revocar tokens de agente; una lista de aprobaciones pendientes en la interfaz. Los tokens de agente no caducan y todavía no hay un comando para revocarlos. Se vuelve a señalar donde importa.

Consulta [Flujos de ejemplo](/docs/agents/workflows/) para prompts y flujos de partida, y [Solución de problemas](/docs/agents/troubleshooting/) cuando algo no funcione.
