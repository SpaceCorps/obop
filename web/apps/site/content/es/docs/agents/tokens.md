---
title: "Crear un token de agente"
description: "Crea un token de agente de obop con alcance limitado mediante REST (POST /api/auth/agent-token) o con la CLI (obop agent-token). El token se muestra una sola vez y solo puede reducir los grupos de permisos de su propietario."
lead: "Un token de agente es un bearer token que pertenece a una persona y lleva una lista de grupos de permisos. Crea uno por agente y por tarea."
---

## Qué es un token

- Pertenece a la persona que lo creó (su _propietario_) y actúa con un subconjunto de los grupos de esa persona.
- Se muestra **una sola vez**. obop solo guarda un hash, así que un token perdido no se puede recuperar; crea uno nuevo.
- Empieza por `obop_`. Trátalo como una contraseña.
- Solo una persona puede crear tokens. Un agente no puede emitir otro token.
- No caduca y deja de funcionar cuando se elimina a su propietario. Todavía no hay un comando para revocar un token concreto (consulta [Seguridad](/docs/agents/safety/)).

## Elige los scopes

Los scopes son [grupos de permisos](/docs/concepts/permission-groups/). Piensa en lo que debe hacer el agente y dale solo esos grupos. Grupos habituales de las aplicaciones incluidas:

| Grupo               | Permite al agente                                                        |
| ------------------- | ------------------------------------------------------------------------ |
| `base.user`         | leer y escribir Contactos (los datos personales siguen enmascarados)     |
| `crm.user`          | leer y escribir oportunidades y moverlas por el embudo                   |
| `sales.user`        | leer y escribir presupuestos y pedidos                                   |
| `invoicing.user`    | leer y escribir facturas y cancelar borradores                           |
| `invoicing.manager` | proponer la contabilización de una factura (una persona sigue aprobando) |
| `product.user`      | leer productos y precios                                                 |

Los paquetes instalados añaden sus propios grupos, por ejemplo `helpdesk.agent`. Las [páginas de aplicaciones](/apps/) indican los grupos de cada una.

## Mediante REST

Inicia sesión como persona y después pide un token. Esto requiere una instancia en marcha; los ejemplos usan una local en el puerto 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

La respuesta es `{"token":"obop_..."}`: un token de sesión que dura 12 horas. Úsalo como bearer token para crear el token de agente:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

`scopes` es obligatorio y debe nombrar al menos un grupo que tú tengas. `label` es texto libre. En una instancia alojada, usa su dirección en lugar de `127.0.0.1:8080` e inicia sesión con el correo de tu registro como nombre de usuario.

## Con la CLI

Si ejecutas obop tú mismo, puedes emitir un token directamente desde la línea de comandos. Habla con la base de datos de forma directa, así que ejecútalo donde la base de datos sea accesible:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Añade `--db postgres://...` (o define `OBOP_DATABASE_URL`) si la instancia no usa el valor por defecto `sqlite://obop.db`.

## Guárdalo de forma segura

- Ponlo en una variable de entorno o en un almacén de secretos, no en un repositorio.
- Es preferible un token por agente y por tarea, para distinguirlos y poder eliminar uno sin afectar a los demás.
- Los tokens en un archivo de configuración de cliente los puede leer cualquier cosa que pueda leer ese archivo.

## Errores

| Respuesta                                                                      | Significado                                     |
| ------------------------------------------------------------------------------ | ----------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                      | `scopes` estaba vacío.                          |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}` | Pediste un grupo que no tienes.                 |
| `400 {"error":"only a human user can issue agent tokens"}`                     | Usaste un token de agente para pedir otro.      |
| `401`                                                                          | Token de sesión ausente, incorrecto o caducado. |

Siguiente: [conectar mediante MCP](/docs/agents/mcp/) o [usar la API REST](/docs/agents/rest/).
