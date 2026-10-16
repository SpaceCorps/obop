---
title: "Erste Schritte"
description: "Ihre erste obop-Instanz: für die gehostete Beta registrieren, mit Docker und PostgreSQL selbst hosten oder lokal mit SQLite betreiben. Danach eine App installieren, Datensätze anlegen und einen KI-Agenten einladen."
lead: "Drei Wege, obop zu betreiben. Wählen Sie die gehostete Beta, um es in wenigen Minuten auszuprobieren, Docker mit PostgreSQL für einen echten Einsatz oder einen lokalen Build für die Entwicklung."
---

## Option 1: die gehostete Beta

Der gehostete Dienst gibt jedem Unternehmen eine eigene Instanz mit eigener Datenbank. Während der Beta ist er kostenlos.

1. **Registrieren.** Öffnen Sie [die Registrierungs-App](/app/), geben Sie Ihre E-Mail-Adresse und ein Passwort ein und wählen Sie _Create account_.
2. **Instanz anlegen.** Vergeben Sie einen Namen mit 3 bis 30 Zeichen. Die Adresse wird aus dem Namen gebildet. Die Einrichtung dauert einen Moment; die Seite zeigt den Fortschritt.
3. **Anmelden.** Folgen Sie dem Link zu Ihrer Instanz und melden Sie sich mit derselben E-Mail-Adresse und demselben Passwort an. Ihr Registrierungskonto wird der erste Administrator der Instanz.
4. **Eine App installieren.** Öffnen Sie _Customize_, dann _Marketplace_, und installieren Sie eine App wie Helpdesk oder Projects. Zum Installieren und Entfernen von Apps ist die Administratorgruppe nötig. Die fünf integrierten Apps (CRM, Sales, Invoicing, Products und Contacts) sind bereits vorhanden.
5. **Datensätze anlegen.** Öffnen Sie eine App, wählen Sie _New_ und füllen Sie das Formular aus. Listen, Boards und die Seitenleiste werden aus der Beschreibung der App erzeugt.
6. **Einen Agenten einladen.** Für Agent-Tokens gibt es noch keine Oberfläche. Melden Sie sich über die REST-API an und [erstellen Sie ein Agent-Token](/docs/agents/tokens/), dann [verbinden Sie sich über MCP](/docs/agents/mcp/) oder nutzen [REST](/docs/agents/rest/).

## Option 2: selbst hosten mit Docker und PostgreSQL

Sie benötigen Docker. Das Repository enthält eine Compose-Datei, die PostgreSQL und obop gemeinsam startet und nur auf localhost veröffentlicht:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

Der erste Build kompiliert den Rust-Workspace und dauert eine Weile. Sobald alles läuft, öffnen Sie `http://127.0.0.1:8080`. Die Compose-Datei startet obop mit Demodaten (`--demo`) und ohne Benutzer, legen Sie also den ersten Administrator an:

```bash
docker compose exec obop obop admin create alice --all-groups
```

obop liest das Passwort von der Standardeingabe, Sie werden also danach gefragt (mindestens 12 Zeichen). Melden Sie sich als `alice` an.

Um das Image stattdessen mit Ihrer eigenen PostgreSQL-Instanz zu betreiben, setzen Sie `OBOP_DATABASE_URL`:

```bash
docker build -t obop .
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://user:password@host:5432/obop \
  obop
```

Das Image betreibt standardmäßig eine Instanz. Sein Befehl lautet `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice`. Unter [Selbsthosting und Betrieb](/docs/reference/self-hosting/) finden Sie die Control Plane, Backups und das, was noch fehlt.

## Option 3: lokale Entwicklung mit SQLite

Sie benötigen die Rust-Toolchain aus `rust-toolchain.toml`, pnpm und Vite+ (`vp`). SQLite ist der Standardspeicher, ein Datenbankserver ist also nicht nötig.

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
cargo build
vp install
(cd web/apps/backoffice && vp build)
echo "a password of 12+ characters" | cargo run -- admin create alice --all-groups
cargo run -- serve --static web/apps/backoffice/dist
```

Öffnen Sie `http://127.0.0.1:8080` und melden Sie sich als `alice` an. Die Daten liegen in `obop.db` im aktuellen Verzeichnis; mit `--db sqlite:///path/to/file.db` verwenden Sie eine andere Datei. Eine neue, leere SQLite-Datenbank wird automatisch mit erfundenen Demodaten gefüllt (`--demo` erzwingt das).

`--dev-auth` ersetzt die echte Anmeldung durch feste Tokens namens `dev-admin`, `dev-sales` und `dev-agent`. Der Server weigert sich dabei, an etwas anderes als localhost zu binden.

## Wie es weitergeht

- Lernen Sie die Bausteine unter [Konzepte](/docs/concepts/) kennen.
- Stöbern Sie in den [Apps](/apps/) und installieren Sie die, die Sie brauchen, über _Customize_ oder auf der Kommandozeile mit `obop modules install-from helpdesk`.
- Richten Sie einen Agenten ein: [Überblick über KI-Agenten](/docs/agents/).
- Schreiben Sie Ihre eigene App: [Referenz zum Modul-Manifest](/docs/reference/module-manifest/).
