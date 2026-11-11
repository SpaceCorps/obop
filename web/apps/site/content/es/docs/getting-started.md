---
title: "Primeros pasos"
description: "Pon en marcha tu primera instancia de obop: autoalójala con Docker y PostgreSQL, o ejecútala en local con SQLite. Después instala una aplicación, crea registros e invita a un agente."
lead: "Dos formas de ejecutar obop: Docker con PostgreSQL para un despliegue real, o una compilación local para desarrollar. obop.dev aún no ofrece instancias alojadas."
---

SpaceCorps aún no aloja obop para otras personas, así que obop lo ejecutas tú. Están previstos un entorno de pruebas (sandbox) gratuito con datos de ejemplo y un alojamiento de pago en servidores dedicados; ninguno de los dos está abierto todavía, y no se promete ninguna fecha. Consulta [Precios](/pricing/).

## Opción 1: autoalojar con Docker y PostgreSQL

Necesitas Docker. El repositorio incluye un archivo Compose que inicia PostgreSQL y obop juntos, publicados solo en localhost:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

La primera compilación construye el workspace de Rust y tarda un buen rato. Cuando esté en marcha, abre `http://127.0.0.1:8080/admin/`. El archivo Compose inicia obop con datos de demostración (`--demo`) y sin usuarios, así que crea el primer administrador:

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

Por defecto, la imagen ejecuta una sola instancia. Su comando es `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice --public-site --trust-proxy`: el sitio público está en `/` y el back office en `/admin/`. `--trust-proxy` toma la dirección del cliente de `X-Forwarded-For`, así que ejecuta la imagen detrás de un proxy que controles. Crea el primer administrador con `docker exec -i obop obop admin create alice --all-groups`. Consulta [Autoalojamiento y operación](/docs/reference/self-hosting/) para el plano de control, las copias de seguridad y lo que falta.

## Opción 2: desarrollo local con SQLite

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

## Después de iniciar sesión

1. **Instala una aplicación.** Abre _Personalizar, Módulos_ y después la pestaña _Mercado_, e instala una aplicación como Servicio de asistencia o Proyectos. Instalar y quitar aplicaciones requiere el grupo de administradores. Las aplicaciones incluidas ya están disponibles, entre ellas Contactos, Productos, CRM, Ventas, Facturación y Conectores (las seis que tienen una página de aplicación).
2. **Crea registros.** Abre una aplicación, elige _Nuevo_ y rellena el formulario. Las listas, los tableros y el panel lateral se generan a partir de la descripción de la aplicación.
3. **Invita a un agente.** Todavía no hay una pantalla para los tokens de agente. Inicia sesión mediante la API REST y [crea un token de agente](/docs/agents/tokens/); después [conéctate mediante MCP](/docs/agents/mcp/) o usa [REST](/docs/agents/rest/).

## Y después

- Conoce los bloques básicos en [Conceptos](/docs/concepts/).
- Explora las [aplicaciones](/apps/) e instala las que necesites desde _Personalizar_, o desde la línea de comandos con `obop modules install-from helpdesk`.
- Configura un agente: [Resumen de agentes](/docs/agents/).
- Escribe tu propia aplicación: [referencia del manifiesto de módulo](/docs/reference/module-manifest/).
