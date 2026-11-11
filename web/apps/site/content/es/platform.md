---
title: "Plataforma"
description: "Cómo funciona la plataforma obop: instancias con una base de datos por negocio, módulos y marketplace, grupos de permisos, clases de datos y enmascarado, aprobaciones, y las superficies MCP, REST y CLI."
lead: "Un programa, un modelo de datos, tres formas de entrar. Esta página explica cómo encajan las piezas."
eyebrow: "Cómo funciona"
---

## Una instancia por negocio

Cada negocio funciona en su propia **instancia**, con su propia base de datos, sus propios usuarios y su propia dirección. En el servicio alojado, un plano de control crea la instancia por ti tras el registro y te da su dirección; la persona que se registró se convierte en su primer administrador. También puedes ejecutar el mismo programa tú mismo. Consulta [Instancias](/docs/concepts/instances/).

El almacenamiento es SQLite para probar y PostgreSQL para un uso real. Por ahora, los registros se guardan como documentos JSON; la capa de comandos comprueba los tipos de los campos y está previsto un gestor de esquemas.

## Los módulos son aplicaciones

Una aplicación es un **módulo**: una descripción de modelos, campos, grupos de permisos y acciones de flujo de trabajo. Seis aplicaciones integradas tienen una página en este sitio (Contacts, Products, CRM, Sales, Invoicing, Connectors). Otras 24 se distribuyen como paquetes, y 22 de ellas también tienen página. Añades aplicaciones desde el marketplace con un clic, o subes tu propio archivo.

Como todos los módulos pasan por un único registro, un contacto es el mismo contacto en CRM, Sales e Invoicing, y cada módulo obtiene la misma API, las mismas pantallas, los mismos permisos y las mismas reglas de privacidad. Consulta [Aplicaciones y módulos](/docs/concepts/apps-and-modules/) y la [lista de aplicaciones](/apps/).

## Marketplace

El marketplace es un host estático que sirve `index.json` y un `obop.toml` por paquete. El catálogo incluido viene dentro del programa; también puedes apuntar una instancia a tu propio host. Cada descarga se valida igual que una subida. Consulta [Marketplace](/marketplace/).

## Permisos

Los grupos de permisos, como `crm.user` o `invoicing.manager`, se nombran en modelos, campos y acciones. Las personas pertenecen a grupos; los tokens de agente tienen un subconjunto de los grupos de su propietario. Consulta [Grupos de permisos](/docs/concepts/permission-groups/).

## Clases de datos y enmascarado

Cada campo declara una clase de datos (ninguna, directorio público, interna, RR. HH. confidencial, categoría especial, financiera, secreta) y si es un dato personal. Las personas ven los valores según sus permisos. Los agentes ven los datos personales como tokens marcadores estables y las clases sensibles no las ven en absoluto, y pueden devolver tokens para referirse a un valor sin verlo. Consulta [Clases de datos y enmascarado](/docs/concepts/data-classes-and-masking/).

## Flujos de trabajo y aprobaciones

Los cambios de estado son acciones con nombre. Una acción puede requerir un grupo más estricto y puede requerir aprobación. Un agente solo puede proponer una acción que requiera aprobación: el servidor devuelve un plan y no cambia nada hasta que una persona lo aprueba. Los agentes también solo pueden proponer la eliminación de registros. Consulta [Flujos de trabajo y aprobaciones](/docs/concepts/workflows-and-approvals/).

## Tres superficies, un mismo conjunto de comandos

| Superficie     | Para                        | Cómo                                                                                                                |
| -------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Aplicación web | Personas                    | Generada a partir de las descripciones de los módulos: listas, tableros, formularios.                               |
| REST           | Scripts, servicios, agentes | `POST /api/{model}/{command}` con un bearer token. [Referencia REST](/docs/reference/rest-api/)                     |
| MCP            | Agentes de IA               | `obop mcp` sobre stdio o `/mcp` sobre HTTP, una herramienta por comando. [Conectar mediante MCP](/docs/agents/mcp/) |
| CLI            | Shells y cron               | `obop call model command key=value`. [Referencia de la CLI](/docs/reference/cli/)                                   |

Todas llaman al mismo despachador, por lo que las comprobaciones de permisos, el enmascarado y las aprobaciones son idénticos.

## Lo que (todavía) no es

Ten en cuenta esto antes de comprometerte:

- No hay niveles de código más allá de los paquetes declarativos (ni componentes WebAssembly ni aplicaciones remotas), por lo que un paquete de aplicación no ejecuta código propio y no puede declarar llamadas a otros sistemas, aunque sí puede declarar totales calculados y acciones que cambian otros registros de la misma base de datos.
- Solo algunas aplicaciones (por ejemplo Expenses, Timesheets, las solicitudes de ausencia de HR, Documents y Dashboards) limitan qué registros ve una persona. CRM, Sales e Invoicing no lo hacen.
- El registro de seguridad recoge, por ejemplo, eventos de cuentas, de inicio de sesión, de ajustes de seguridad, de módulos y de firmas, pero no es un historial de los cambios en los registros del negocio.
- La tarea de exportación prevista para todo un negocio no está construida. Una lista se exporta como CSV desde la aplicación web (hasta 20 000 registros), y `obop backup --plain` escribe los registros y los archivos subidos de una base de datos en un archivo sin cifrar (los valores de corta duración, como las sesiones, se dejan fuera, y los secretos siguen sellados).
- El servicio alojado aún no activa las copias de seguridad programadas de sus bases de datos (quien ejecuta obop por su cuenta puede activarlas).

La [hoja de ruta](/roadmap/) indica qué está construido y qué no.
