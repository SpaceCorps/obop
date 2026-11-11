---
title: "Kom igång"
description: "Kör din första obop-instans: kör själv med Docker och PostgreSQL eller kör lokalt med SQLite. Installera sedan en app, skapa poster och bjud in en agent."
lead: "Två sätt att köra obop: Docker med PostgreSQL för en skarp installation eller ett lokalt bygge för utveckling. obop.dev erbjuder ännu inga hostade instanser."
---

SpaceCorps hostar ännu inte obop åt andra, så du kör obop själv. En gratis sandlåda med exempeldata och betald hosting på dedikerade servrar är planerade; ingen av dem är öppen än, och inget datum utlovas. Se [Priser](/pricing/).

## Alternativ 1: egen drift med Docker och PostgreSQL

Du behöver Docker. Arkivet innehåller en Compose-fil som startar PostgreSQL och obop tillsammans, publicerade enbart på localhost:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

Det första bygget kompilerar Rust-arbetsytan och tar en stund. När det är klart öppnar du `http://127.0.0.1:8080/admin/`. Compose-filen startar obop med demodata (`--demo`) och utan användare, så skapa den första administratören:

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

Avbilden kör som standard en instans. Dess kommando är `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice --public-site --trust-proxy`: den publika webbplatsen ligger på `/` och back office på `/admin/`. `--trust-proxy` läser klientens adress från `X-Forwarded-For`, så kör avbilden bakom en proxy som du själv kontrollerar. Skapa den första administratören med `docker exec -i obop obop admin create alice --all-groups`. Se [Egen drift och underhåll](/docs/reference/self-hosting/) för kontrollplanet, säkerhetskopior och vad som saknas.

## Alternativ 2: lokal utveckling med SQLite

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

## När du har loggat in

1. **Installera en app.** Öppna _Anpassa, Moduler_ och fliken _Marknadsplats_, och installera en app som Kundtjänst eller Projekt. För att installera och ta bort appar krävs administratörsgruppen. De inbyggda apparna finns redan, bland dem Kontakter, Produkter, CRM, Försäljning, Fakturering och Kopplingar (de sex som har en appsida).
2. **Skapa poster.** Öppna en app, välj _Ny_ och fyll i formuläret. Listor, tavlor och sidopanelen genereras från appens beskrivning.
3. **Bjud in en agent.** Det finns ännu ingen skärmbild för agenttoken. Logga in via REST-API:et och [skapa en agenttoken](/docs/agents/tokens/), och [anslut sedan via MCP](/docs/agents/mcp/) eller använd [REST](/docs/agents/rest/).

## Vad händer härnäst

- Lär dig byggstenarna i [Grundbegrepp](/docs/concepts/).
- Bläddra bland [apparna](/apps/) och installera de du behöver från _Anpassa_, eller från kommandoraden med `obop modules install-from helpdesk`.
- Ställ in en agent: [Översikt över AI-agenter](/docs/agents/).
- Skriv din egen app: [referens för modulmanifestet](/docs/reference/module-manifest/).
