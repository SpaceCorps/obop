---
title: "Kom igång"
description: "Skaffa din första obop-instans: registrera dig för den hostade betan, kör själv med Docker och PostgreSQL eller kör lokalt med SQLite. Installera sedan en app, skapa poster och bjud in en agent."
lead: "Tre sätt att köra obop. Välj den hostade betan för att prova på några minuter, Docker med PostgreSQL för en skarp installation eller ett lokalt bygge för utveckling."
---

## Alternativ 1: den hostade betan

Den hostade tjänsten ger varje företag en egen instans med en egen databas. Den är gratis under betan.

1. **Registrera dig.** Öppna [registreringsappen](/app/), ange din e-postadress och ett lösenord och välj _Create account_.
2. **Skapa en instans.** Ge den ett namn på 3 till 30 tecken. Adressen bildas av namnet. Installationen tar en liten stund; sidan visar hur det går.
3. **Logga in.** Följ länken till din instans och logga in med samma e-postadress och lösenord. Ditt registreringskonto blir instansens första administratör.
4. **Installera en app.** Öppna _Customize_, sedan _Marketplace_, och installera en app som Helpdesk eller Projects. För att installera och ta bort appar krävs administratörsgruppen. De fem inbyggda apparna (CRM, Sales, Invoicing, Products och Contacts) finns redan.
5. **Skapa poster.** Öppna en app, välj _New_ och fyll i formuläret. Listor, tavlor och sidopanelen genereras från appens beskrivning.
6. **Bjud in en agent.** Det finns ännu ingen skärmbild för agenttoken. Logga in via REST-API:et och [skapa en agenttoken](/docs/agents/tokens/), och [anslut sedan via MCP](/docs/agents/mcp/) eller använd [REST](/docs/agents/rest/).

## Alternativ 2: egen drift med Docker och PostgreSQL

Du behöver Docker. Arkivet innehåller en Compose-fil som startar PostgreSQL och obop tillsammans, publicerade enbart på localhost:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

Det första bygget kompilerar Rust-arbetsytan och tar en stund. När det är klart öppnar du `http://127.0.0.1:8080`. Compose-filen startar obop med demodata (`--demo`) och utan användare, så skapa den första administratören:

```bash
docker compose exec obop obop admin create alice --all-groups
```

obop läser lösenordet från standardinmatningen, så du får en fråga om det (minst 12 tecken). Logga in som `alice`.

Om du vill köra avbilden mot din egen PostgreSQL-instans i stället sätter du `OBOP_DATABASE_URL`:

```bash
docker build -t obop .
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://user:password@host:5432/obop \
  obop
```

Avbilden kör som standard en instans. Dess kommando är `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice`. Se [Egen drift och underhåll](/docs/reference/self-hosting/) för kontrollplanet, säkerhetskopior och vad som saknas.

## Alternativ 3: lokal utveckling med SQLite

Du behöver Rust-verktygskedjan från `rust-toolchain.toml`, pnpm och Vite+ (`vp`). SQLite är standardlagring, så ingen databasserver behövs.

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
cargo build
vp install
(cd web/apps/backoffice && vp build)
echo "a password of 12+ characters" | cargo run -- admin create alice --all-groups
cargo run -- serve --static web/apps/backoffice/dist
```

Öppna `http://127.0.0.1:8080` och logga in som `alice`. Datan ligger i `obop.db` i den aktuella katalogen; ange `--db sqlite:///path/to/file.db` för att använda en annan fil. En ny, tom SQLite-databas fylls automatiskt med påhittad demodata (`--demo` tvingar fram det).

`--dev-auth` ersätter riktig inloggning med fasta token som heter `dev-admin`, `dev-sales` och `dev-agent`. Det vägrar att binda till något annat än localhost.

## Vad händer härnäst

- Lär dig byggstenarna i [Grundbegrepp](/docs/concepts/).
- Bläddra bland [apparna](/apps/) och installera de du behöver från _Customize_, eller från kommandoraden med `obop modules install-from helpdesk`.
- Ställ in en agent: [Översikt över AI-agenter](/docs/agents/).
- Skriv din egen app: [referens för modulmanifestet](/docs/reference/module-manifest/).
