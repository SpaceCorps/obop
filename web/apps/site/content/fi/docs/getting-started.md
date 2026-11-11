---
title: "Aloitusopas"
description: "Ota käyttöön ensimmäinen obop-instanssisi: aja alustaa itse Dockerilla ja PostgreSQL:llä tai käytä sitä paikallisesti SQLitellä. Asenna sen jälkeen sovellus, luo tietueita ja kutsu agentti mukaan."
lead: "obopia voi ajaa kahdella tavalla: Docker ja PostgreSQL sopivat oikeaan käyttöön, ja paikallinen versio on tarkoitettu kehitykseen. obop.dev ei vielä tarjoa isännöityjä instansseja."
---

SpaceCorps ei vielä isännöi obopia muille, joten ajat obopia itse. Ilmainen hiekkalaatikko (sandbox) esimerkkitiedoilla ja maksullinen isännöinti dedikoiduilla palvelimilla ovat suunnitteilla; kumpikaan ei ole vielä avoinna, eikä mitään päivämäärää luvata. Katso [Hinnoittelu](/pricing/).

## Vaihtoehto 1: aja itse Dockerilla ja PostgreSQL:llä

Tarvitset Dockerin. Repositorion Compose-tiedosto käynnistää PostgreSQL:n ja obopin yhdessä, ja ne julkaistaan vain localhostissa:

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

Ensimmäinen koontiversio kääntää Rust-työtilan ja kestää hetken. Kun palvelu on käynnissä, avaa `http://127.0.0.1:8080/admin/`. Compose-tiedosto käynnistää obopin demotiedoilla (`--demo`) ja ilman käyttäjiä, joten luo ensimmäinen ylläpitäjä:

```bash
docker compose exec obop obop admin create alice --all-groups
```

obop lukee salasanan vakiosyötteestä, joten sitä kysytään sinulta (vähintään 12 merkkiä). Kirjaudu sisään tunnuksella `alice`.

Jos haluat ajaa imagea oman PostgreSQL-tietokantasi kanssa, aseta `OBOP_DATABASE_URL`:

```bash
docker build -t obop .
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://user:password@host:5432/obop \
  obop
```

Image ajaa oletuksena yhtä instanssia. Sen komento on `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice --public-site --trust-proxy`: julkinen sivusto on osoitteessa `/` ja hallintanäkymä osoitteessa `/admin/`. `--trust-proxy` lukee asiakkaan osoitteen otsakkeesta `X-Forwarded-For`, joten aja imagea itse hallitsemasi välityspalvelimen takana. Luo ensimmäinen ylläpitäjä komennolla `docker exec -i obop obop admin create alice --all-groups`. Ohjaustaso, varmuuskopiot ja puuttuvat ominaisuudet on kuvattu sivulla [Oma ylläpito ja käyttö](/docs/reference/self-hosting/).

## Vaihtoehto 2: paikallinen kehitys SQLitellä

Tarvitset Rust-työkalut tiedoston `rust-toolchain.toml` mukaisesti, pnpm:n ja Vite+:n (`vp`). SQLite on oletustallennus, joten tietokantapalvelinta ei tarvita.

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
cargo build
vp install
(cd web/apps/backoffice && vp build)
echo "a password of 12+ characters" | cargo run -- admin create alice --all-groups
cargo run -- serve --static web/apps/backoffice/dist
```

Avaa `http://127.0.0.1:8080` ja kirjaudu sisään tunnuksella `alice`. Tiedot tallentuvat nykyisen hakemiston tiedostoon `obop.db`; toisen tiedoston voi ottaa käyttöön valitsimella `--db sqlite:///path/to/file.db`. Uusi, tyhjä SQLite-tietokanta täytetään automaattisesti keksityillä demotiedoilla (`--demo` pakottaa täytön).

`--dev-auth` korvaa oikean kirjautumisen kiinteillä tunnisteilla `dev-admin`, `dev-sales` ja `dev-agent`. Se kieltäytyy sitoutumasta muuhun kuin localhostiin.

## Kun olet kirjautunut sisään

1. **Asenna sovellus.** Avaa _Mukauta, Moduulit_ ja siellä välilehti _Moduulikauppa_, ja asenna sovellus, esimerkiksi Asiakastuki tai Projektit. Sovellusten asentaminen ja poistaminen vaatii ylläpitäjän ryhmän. Sisäänrakennetut sovellukset ovat jo valmiina, muun muassa Yhteystiedot, Tuotteet, CRM, Myynti, Laskutus ja Liittimet (ne kuusi, joilla on oma sovellussivu).
2. **Luo tietueita.** Avaa sovellus, valitse _Uusi_ ja täytä lomake. Luettelot, taulut ja sivupaneeli muodostetaan sovelluksen kuvauksesta.
3. **Kutsu agentti.** Agenttitunnisteille ei ole vielä omaa näkymää. Kirjaudu sisään REST-rajapinnan kautta ja [luo agenttitunniste](/docs/agents/tokens/). Sen jälkeen voit [yhdistää MCP:n kautta](/docs/agents/mcp/) tai käyttää [REST-rajapintaa](/docs/agents/rest/).

## Mitä seuraavaksi

- Tutustu rakennuspalikoihin sivulla [Peruskäsitteet](/docs/concepts/).
- Selaa [sovelluksia](/apps/) ja asenna tarvitsemasi _Mukauta_-sovelluksesta tai komentoriviltä komennolla `obop modules install-from helpdesk`.
- Ota agentti käyttöön: [Agenttien yleiskatsaus](/docs/agents/).
- Kirjoita oma sovellus: [moduulin manifestiviite](/docs/reference/module-manifest/).
