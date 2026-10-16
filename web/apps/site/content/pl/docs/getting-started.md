---
title: "Pierwsze kroki"
description: "Uruchom pierwszą instancję obop: zarejestruj się w hostowanej wersji beta, uruchom system samodzielnie w Dockerze z PostgreSQL albo lokalnie z SQLite. Potem zainstaluj aplikację, utwórz rekordy i zaproś agenta."
lead: "Trzy sposoby uruchomienia obop. Wybierz hostowaną betę, aby wypróbować system w kilka minut, Dockera z PostgreSQL do prawdziwego wdrożenia albo lokalną kompilację do pracy deweloperskiej."
---

## Opcja 1: hostowana beta

Usługa hostowana daje każdej firmie własną instancję z własną bazą danych. W czasie bety jest bezpłatna. Nazwy elementów interfejsu (_Create account_, _Customize_, _Marketplace_, _New_) podajemy tak, jak widać je w aplikacji, czyli po angielsku.

1. **Zarejestruj się.** Otwórz [aplikację rejestracji](/app/), wpisz adres e-mail i hasło, a następnie wybierz _Create account_.
2. **Utwórz instancję.** Nadaj jej nazwę o długości od 3 do 30 znaków. Adres powstaje z nazwy. Przygotowanie trwa chwilę; strona pokazuje postęp.
3. **Zaloguj się.** Przejdź do odnośnika do swojej instancji i zaloguj się tym samym adresem e-mail i hasłem. Konto utworzone przy rejestracji zostaje pierwszym administratorem instancji.
4. **Zainstaluj aplikację.** Otwórz _Customize_, następnie _Marketplace_ i zainstaluj aplikację, na przykład Helpdesk lub Projects. Instalowanie i usuwanie aplikacji wymaga grupy administratora. Pięć wbudowanych aplikacji (CRM, Sales, Invoicing, Products i Contacts) jest już dostępnych.
5. **Utwórz rekordy.** Otwórz aplikację, wybierz _New_ i wypełnij formularz. Listy, tablice i panel boczny są generowane z opisu aplikacji.
6. **Zaproś agenta.** Nie ma jeszcze ekranu do tokenów agentów. Zaloguj się przez REST API i [utwórz token agenta](/docs/agents/tokens/), a potem [połącz się przez MCP](/docs/agents/mcp/) lub użyj [REST](/docs/agents/rest/).

## Opcja 2: własny hosting w Dockerze z PostgreSQL

Potrzebujesz Dockera. Repozytorium zawiera plik Compose, który uruchamia razem PostgreSQL i obop, udostępnione wyłącznie na localhost:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

Pierwsza kompilacja obejmuje cały workspace w Rust i trwa dość długo. Gdy system się uruchomi, otwórz `http://127.0.0.1:8080`. Plik Compose uruchamia obop z danymi demonstracyjnymi (`--demo`) i bez użytkowników, więc utwórz pierwszego administratora:

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

Obraz domyślnie uruchamia jedną instancję. Jego polecenie to `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice`. Płaszczyznę sterowania, kopie zapasowe i to, czego jeszcze brakuje, opisuje strona [Własny hosting i eksploatacja](/docs/reference/self-hosting/).

## Opcja 3: praca lokalna z SQLite

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

Otwórz `http://127.0.0.1:8080` i zaloguj się jako `alice`. Dane znajdują się w pliku `obop.db` w bieżącym katalogu; aby użyć innego pliku, przekaż `--db sqlite:///path/to/file.db`. Nowa, pusta baza SQLite jest automatycznie wypełniana fikcyjnymi danymi demonstracyjnymi (`--demo` wymusza to zawsze).

`--dev-auth` zastępuje prawdziwe logowanie stałymi tokenami `dev-admin`, `dev-sales` i `dev-agent`. Odmawia działania na jakimkolwiek adresie poza localhost.

## Co dalej

- Poznaj elementy składowe w części [Pojęcia](/docs/concepts/).
- Przejrzyj [aplikacje](/apps/) i zainstaluj potrzebne z _Customize_ albo z wiersza poleceń poleceniem `obop modules install-from helpdesk`.
- Skonfiguruj agenta: [przegląd agentów](/docs/agents/).
- Napisz własną aplikację: [opis manifestu modułu](/docs/reference/module-manifest/).
