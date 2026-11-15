---
title: "Pierwsze kroki"
description: "Uruchom pierwszą instancję obop: samodzielnie w Dockerze z PostgreSQL albo lokalnie z SQLite. Potem zainstaluj aplikację, utwórz rekordy i zaproś agenta."
lead: "Dwa sposoby uruchomienia obop: w Dockerze z PostgreSQL albo z lokalnej kompilacji do pracy deweloperskiej. SpaceCorps nie hostuje jeszcze obop dla innych."
---

SpaceCorps nie hostuje jeszcze obop dla innych, więc uruchamiasz obop samodzielnie. Planowane są bezpłatna piaskownica (sandbox) z przykładowymi danymi i płatny hosting na serwerach dedykowanych; żadne z nich nie jest jeszcze dostępne i nie obiecujemy żadnego terminu. Zobacz [Cennik](/pricing/) i [plan rozwoju](/roadmap/).

## Opcja 1: własny hosting w Dockerze z PostgreSQL

Potrzebujesz Dockera. Repozytorium zawiera plik Compose do lokalnego wypróbowania, który uruchamia razem PostgreSQL i obop, udostępnione wyłącznie na localhost:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

Pierwsza kompilacja obejmuje cały workspace w Rust i trwa dość długo. Gdy system się uruchomi, otwórz `http://127.0.0.1:8080/admin/`. Plik Compose uruchamia obop z danymi demonstracyjnymi (`--demo`) i bez użytkowników, więc utwórz pierwszego administratora:

```bash
docker compose exec obop obop admin create alice --all-groups
```

Hasło jest odczytywane ze standardowego wejścia, więc zostanie wyświetlony monit o jego podanie (co najmniej 12 znaków). Zaloguj się jako `alice`.

Aby uruchomić obraz z własną instancją PostgreSQL, ustaw `OBOP_DATABASE_URL`:

```bash
docker build -t obop .
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://user:password@host:5432/obop \
  obop
```

Obraz domyślnie uruchamia jedną instancję. Jego polecenie to `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice --public-site --trust-proxy`: publiczna strona jest pod `/`, a panel administracyjny pod `/admin/`. `--trust-proxy` odczytuje adres klienta z nagłówka `X-Forwarded-For`, więc uruchamiaj obraz za serwerem proxy, który sam kontrolujesz. Pierwszego administratora utworzysz poleceniem `docker exec -i obop obop admin create alice --all-groups`. Płaszczyznę sterowania, kopie zapasowe i to, czego jeszcze brakuje, opisuje strona [Własny hosting i eksploatacja](/docs/reference/self-hosting/).

## Opcja 2: praca lokalna z SQLite

Potrzebujesz zestawu narzędzi Rust z pliku `rust-toolchain.toml`, pnpm oraz Vite+ (`vp`). Domyślnym magazynem jest SQLite, więc serwer bazy danych nie jest potrzebny.

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
cargo build
vp install
(cd web/apps/backoffice && vp build)
echo "a password of 12+ characters" | cargo run -- admin create alice --all-groups
cargo run -- serve --static web/apps/backoffice/dist
```

Otwórz `http://127.0.0.1:8080` i zaloguj się jako `alice`. Dane znajdują się w pliku `obop.db` w bieżącym katalogu; aby użyć innego pliku, przekaż `--db sqlite:///path/to/file.db`. Baza SQLite, która nie ma jeszcze żadnych kontaktów, jest automatycznie wypełniana fikcyjnymi danymi demonstracyjnymi; w PostgreSQL to samo robi `--demo`.

`--dev-auth` zastępuje prawdziwe logowanie stałymi tokenami `dev-admin`, `dev-sales` i `dev-agent`. Odmawia działania na jakimkolwiek adresie poza localhost.

## Po zalogowaniu

Nazwy elementów interfejsu podajemy tak, jak wyświetla je aplikacja w języku polskim.

1. **Zainstaluj aplikację.** Otwórz aplikację _Dostosowanie_, w niej _Moduły_ i kartę _Marketplace_, a potem zainstaluj aplikację, na przykład Helpdesk lub Projekty. Instalowanie i usuwanie aplikacji wymaga grupy administratora. Wbudowane aplikacje są już dostępne, a wśród nich Kontakty, Produkty, CRM, Sprzedaż, Fakturowanie i Konektory (sześć, które mają stronę aplikacji).
2. **Utwórz rekordy.** Otwórz aplikację, wybierz _Nowy_ i wypełnij formularz. Listy, tablice i panel boczny są generowane z opisu aplikacji.
3. **Zaproś agenta.** Nie ma jeszcze ekranu do tokenów agentów. Zaloguj się przez REST API i [utwórz token agenta](/docs/agents/tokens/), a potem [połącz się przez MCP](/docs/agents/mcp/) lub użyj [REST](/docs/agents/rest/).

## Co dalej

- Poznaj elementy składowe w części [Pojęcia](/docs/concepts/).
- Przejrzyj [aplikacje](/apps/) i zainstaluj potrzebne w aplikacji _Dostosowanie_ albo z wiersza poleceń poleceniem `obop modules install-from helpdesk`.
- Skonfiguruj agenta: [przegląd agentów](/docs/agents/).
- Napisz własną aplikację: [opis manifestu modułu](/docs/reference/module-manifest/).
