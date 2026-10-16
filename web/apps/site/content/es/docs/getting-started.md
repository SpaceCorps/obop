---
title: "Primeros pasos"
description: "Obtén tu primera instancia de obop: regístrate en la beta alojada, autoaloja con Docker y PostgreSQL, o ejecútala en local con SQLite. Después instala una aplicación, crea registros e invita a un agente."
lead: "Tres formas de ejecutar obop. Elige la beta alojada para probarlo en minutos, Docker con PostgreSQL para un despliegue real, o una compilación local para desarrollar."
---

## Opción 1: la beta alojada

El servicio alojado da a cada negocio su propia instancia con su propia base de datos. Es gratis durante la beta.

1. **Regístrate.** Abre [la aplicación de registro](/app/), introduce tu correo electrónico y una contraseña, y elige _Create account_.
2. **Crea una instancia.** Ponle un nombre de entre 3 y 30 caracteres. La dirección se genera a partir del nombre. La preparación tarda un poco; la página muestra el progreso.
3. **Inicia sesión.** Sigue el enlace a tu instancia e inicia sesión con el mismo correo y la misma contraseña. Tu cuenta de registro se convierte en el primer administrador de la instancia.
4. **Instala una aplicación.** Abre _Customize_, luego _Marketplace_, e instala una aplicación como Helpdesk o Projects. Instalar y quitar aplicaciones requiere el grupo de administradores. Las cinco aplicaciones incluidas (CRM, Sales, Invoicing, Products y Contacts) ya están disponibles.
5. **Crea registros.** Abre una aplicación, elige _New_ y rellena el formulario. Las listas, los tableros y el panel lateral se generan a partir de la descripción de la aplicación.
6. **Invita a un agente.** Todavía no hay una pantalla para los tokens de agente. Inicia sesión mediante la API REST y [crea un token de agente](/docs/agents/tokens/); después [conéctate mediante MCP](/docs/agents/mcp/) o usa [REST](/docs/agents/rest/).

## Opción 2: autoalojar con Docker y PostgreSQL

Necesitas Docker. El repositorio incluye un archivo Compose que inicia PostgreSQL y obop juntos, publicados solo en localhost:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

La primera compilación construye el workspace de Rust y tarda un buen rato. Cuando esté en marcha, abre `http://127.0.0.1:8080`. El archivo Compose inicia obop con datos de demostración (`--demo`) y sin usuarios, así que crea el primer administrador:

```bash
docker compose exec obop obop admin create alice --all-groups
```

obop lee la contraseña de la entrada estándar, por lo que te la pedirá (al menos 12 caracteres). Inicia sesión como `alice`.

Para ejecutar la imagen contra tu propia instancia de PostgreSQL, define `OBOP_DATABASE_URL`:

```bash
docker build -t obop .
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://user:password@host:5432/obop \
  obop
```

Por defecto, la imagen ejecuta una sola instancia. Su comando es `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice`. Consulta [Autoalojamiento y operación](/docs/reference/self-hosting/) para el plano de control, las copias de seguridad y lo que falta.

## Opción 3: desarrollo local con SQLite

Necesitas la toolchain de Rust indicada en `rust-toolchain.toml`, pnpm y Vite+ (`vp`). SQLite es el almacenamiento por defecto, así que no hace falta un servidor de base de datos.

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
cargo build
vp install
(cd web/apps/backoffice && vp build)
echo "a password of 12+ characters" | cargo run -- admin create alice --all-groups
cargo run -- serve --static web/apps/backoffice/dist
```

Abre `http://127.0.0.1:8080` e inicia sesión como `alice`. Los datos están en `obop.db`, en el directorio actual; pasa `--db sqlite:///path/to/file.db` para usar otro archivo. Una base de datos SQLite nueva y vacía se rellena automáticamente con datos de demostración ficticios (`--demo` lo fuerza).

`--dev-auth` sustituye el inicio de sesión real por tokens fijos llamados `dev-admin`, `dev-sales` y `dev-agent`. Se niega a enlazarse a cualquier dirección que no sea localhost.

## Y después

- Conoce los bloques básicos en [Conceptos](/docs/concepts/).
- Explora las [aplicaciones](/apps/) e instala las que necesites desde _Customize_, o desde la línea de comandos con `obop modules install-from helpdesk`.
- Configura un agente: [Resumen de agentes](/docs/agents/).
- Escribe tu propia aplicación: [referencia del manifiesto de módulo](/docs/reference/module-manifest/).
