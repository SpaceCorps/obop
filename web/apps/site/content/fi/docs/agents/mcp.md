---
title: "Yhdistä MCP:n kautta"
description: "Yhdistä Claude Code, Cursor, VS Code, Windsurf, Codex tai Gemini CLI obop-instanssiin osoitteella etä-MCP-palvelimen kautta (Streamable HTTP, Bearer-tunniste) tai aja obop mcp stdio-yhteydellä. Tarkat asetukset kullekin asiakkaalle."
lead: "Jokaisella obop-instanssilla on MCP-palvelin osoitteessa /mcp. Osoita asiakas URL-osoitteeseen agenttitunnisteella, niin jokainen komento, jota tunniste saa käyttää, muuttuu MCP-työkaluksi: paikallista ohjelmaa ei tarvita."
---

## Etä-MCP: yhdistä osoitteella

Instanssi tarjoaa [Model Context Protocolin](https://modelcontextprotocol.io/) **Streamable HTTP** -siirrolla osoitteessa `https://YOUR-INSTANCE/mcp`. Isännöidyllä instanssilla (esimerkiksi Sliplanessa) ja paikallisella `obop serve` -komennolla se on käytössä; mitään ei tarvitse kytkeä päälle.

- **Todennus:** selainkirjautuminen (OAuth, katso alla) Claude Desktopille, claude.ai:lle ja Claude Codelle tai `Authorization: Bearer <token>` jokaisessa pyynnössä agenttitunnisteella. Tunniste tarkistetaan kummassakin tapauksessa kuten REST-rajapinnassa. Puuttuva tai virheellinen tunniste saa vastauksen `401` ja `WWW-Authenticate: Bearer resource_metadata="https://YOUR-INSTANCE/.well-known/oauth-protected-resource/mcp"`, kuten MCP:n valtuutusmäärittely edellyttää; kyseinen dokumentti nimeää instanssin valtuutuspalvelimen (`/.well-known/oauth-authorization-server`). Kirjautumisella annettu tunniste toimii vain osoitteessa `/mcp`, ei osoitteessa `/api`.
- **Protokollaversiot:** `2025-11-25`, `2025-06-18` ja `2025-03-26`, neuvoteltu `initialize`-kutsussa. Uusin versio, `2026-07-28`, on tilaton ja toimii eri tavalla; obop ei tue sitä vielä. Asiakas, joka puhuu molempia, palaa `initialize`-kutsuun, kun se saa pelkän `400`-vastauksen, jonka obop lähettää tukemattomasta versio-otsakkeesta.
- **Istunnot:** `initialize` palauttaa `Mcp-Session-Id`-arvon; lähetä se jokaisessa myöhemmässä pyynnössä. Istunto säilyttää maskausholvin, joten maskitunniste, kuten `NAME_1`, tarkoittaa samaa istunnon jokaisessa kutsussa ja voit lähettää sen takaisin. Toinen istunto aloittaa uuden holvin. `DELETE /mcp` päättää istunnon; tuntematon tai vanhentunut istunto vastaa `404`, ja asiakas aloittaa uudelleen `initialize`-kutsulla. Istunto kuuluu sen avanneelle tunnisteelle ja vanhenee kahden tunnin kuluttua ilman pyyntöä.
- **Vastaukset ovat pelkkää JSONia.** Palvelimelta asiakkaalle ei ole SSE-virtaa (`GET /mcp` vastaa `405`), ja JSON-RPC-eriä hyväksytään vain versiossa `2025-03-26`, ainoassa versiossa, jossa ne ovat.
- **Rajat:** pyyntöjen rungot enintään 1 MiB (`413`), 600 pyyntöä minuutissa tunnistetta kohti (`429` ja `Retry-After`, säädettävissä muuttujalla `OBOP_MCP_RATE_PER_MINUTE`), 25 avointa istuntoa tunnistetta kohti.
- **Selaimet:** pyyntö, jossa on `Origin`-otsake, hylätään vastauksella `403`, ellei origin ole instanssin oma julkinen URL (`OBOP_PUBLIC_URL`), sama origin loopback-isännällä tai lueteltu muuttujassa `OBOP_ALLOWED_ORIGINS` (pilkuilla erotettuna). Tämä suojaa paikallisia instansseja DNS rebinding -hyökkäyksiltä. Asiakkaat, jotka eivät ole selaimia, eivät lähetä `Origin`-otsaketta. Välityspalvelimen takana aseta `OBOP_PUBLIC_URL` (esimerkiksi `https://acme.example.com`), jotta löydettävyysdokumentit nimeävät julkisen osoitteen.

### Yhdistä selainkirjautumisella (OAuth)

Claude Desktop, claude.ai ja Claude Code voivat yhdistää instanssiin ilman liitettyä tunnistetta. Lisää sovelluksessa mukautettu liitin (custom connector) osoitteella `https://YOUR-INSTANCE/mcp` (Claude Code: `claude mcp add --transport http obop https://YOUR-INSTANCE/mcp`, sitten `/mcp` ja valitse tunnistautuminen). Sovellus löytää kirjautumisen itse ja avaa selaimeen **oman instanssisi** sivun. Siellä:

1. kirjaudut obop-tililläsi ja annat kaksivaiheisen koodin, jos tilillä on sellainen (myös pääsyavain käy, toisena vaiheena tai yksinään, jos sallit sen kohdassa Turvallisuus);
2. näet, mikä sovellus pyytää ja mihin osoitteeseen selain palaa;
3. valitset luovutettavat käyttöoikeusryhmät, valitset pääsyn keston (1, 7, 30 tai 90 päivää) ja painat **Salli**.

Mitä tämä tarkoittaa:

- **Sovellus voi vain kaventaa oikeuksiasi.** Tarjolla ovat ryhmät, jotka itse omistat; sovellus toimii puolestasi valitsemillasi ryhmillä, ei koskaan laajemmin ([agenttitunnisteet](/docs/agents/tokens/) toimivat samoin). Sen toimet tallennetaan agenttisi toimina.
- **Hyväksynnät vaativat edelleen ihmisen.** Hyväksyntää vaativa vaihe odottaa obopissa ihmistä; sovellus ei voi hyväksyä omia ehdotuksiaan.
- **Lyhytikäiset tunnisteet.** Sovellus saa tunnisteen, joka on voimassa tunnin, ja uusintatunnisteen, jota käytetään kerran ja joka vaihtuu joka kerta. Jos uusintatunnistetta käytetään kahdesti, koko yhteys päättyy. Tämän kirjautumisen pääsytunniste toimii vain osoitteessa `/mcp`.
- **Pysyt hallinnassa.** Kohdassa _Mukauta, Turvallisuus, Yhdistetyt sovellukset_ näkyy jokainen yhteys ryhmineen ja päättymispäivineen; **Peruuta** päättää sen heti. Yhdistäminen, kieltäytyminen, peruutus ja kahdesti käytetty tunniste näkyvät turvallisuuslokissa.
- **Mitään ei luoteta etukäteen.** Kuka tahansa sovellus voi rekisteröityä itse, joten sivu näyttää sen valitseman nimen ja paluuosoitteen ja kertoo, ettei obop voi tarkistaa, kuka sitä ylläpitää. Paluuosoitteiden on oltava `https` (tai `http` osoitteissa `localhost`, `127.0.0.1` tai `[::1]`), rekisteröinnit on rajattu ja käyttämättömät vanhenevat.

Protokolla asiakkaiden tekijöille: RFC 9728- ja RFC 8414 -metatiedot, dynaaminen asiakasrekisteröinti (RFC 7591, julkiset asiakkaat), valtuutuskoodi PKCE **S256** -menetelmällä (pakollinen), `resource`-parametri (RFC 8707, sen on oltava instanssin `/mcp`-osoite), uusintatunnisteiden kierto uudelleenkäytön tunnistuksella, peruutus (RFC 7009) ja `iss`-parametri uudelleenohjauksessa (RFC 9207). Päätepisteet: `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`. Loopback-uudelleenohjaus (`http://127.0.0.1:PORT/...`) saa käyttää mitä tahansa porttia (RFC 8252); kaikkien muiden on täsmättävä tarkasti. Välityspalvelimen takana oleva instanssi tarvitsee `OBOP_PUBLIC_URL`-asetuksen, jotta näiden dokumenttien osoitteet ovat julkiset. Rakensimme tämän määrittelyn mukaan ja testasimme omalla asiakkaallamme automaattisissa testeissä; emme ole **vielä yhdistäneet** siihen oikeita Claude-sovelluksia.

### Työkalut

`tools/list` näyttää yhden työkalun jokaista komentoa kohti, jota tunniste saa käyttää, nimellä `<model>__<command>`: `party__search`, `lead__create`, `invoice__post`. Jokaisella on mallista generoitu JSON Schema -muotoinen `inputSchema`: rahamäärä- ja lukukentät ovat kokonaislukuja, päivämäärät ovat `format: date`, valintakentät ovat vaihtoehtojensa `enum`, viittauskenttä on merkkijono, joka nimeää kohdemallin, ja pakolliset kentät ovat kohdassa `required`. `list` ja `search` ottavat parametrit `filter`, `order`, `limit` ja `offset`; `get`, `delete` ja toiminnot ottavat parametrin `id`. Merkinnät osoittavat lukemisen (`readOnlyHint`) ja poiston (`destructiveHint`). Yksi lisätyökalu, vain luku -tyyppinen `obop__meta`, palauttaa saman kuin `GET /api/meta`: aloita siitä. Työkalua, jota tunniste ei saa käyttää, ei listata, ja se vastaa kuin työkalua, jota ei ole olemassa.

Tuloksissa on `structuredContent` ja sama JSON tekstinä:

```json
{ "status": "ok", "value": [{ "id": "...", "name": "NAME_1" }], "denied": ["party.tax_id"] }
```

- Vaihe, joka vaatii ihmisen (jokainen poisto, laskun kirjaaminen), ei suoritu agentille. Tulos on tavallinen tulos, jossa on `"status": "approval_required"`, `plan_id` ja ohje pyytää ihmistä, joka hyväksyy sen toimistonäkymässä tai komennolla `POST /api/approve/<plan_id>`. Katso [Hyväksyntäkierros](/docs/agents/approvals/).
- Hylätty syöte (virheellinen arvo, puuttuva kenttä) on työkaluvirhe: `isError: true` ja palvelimen viesti kohdassa `structuredContent.error`.
- `initialize` palauttaa myös `instructions`-kentän, lyhyen tekstin, joka kertoo mallille, miten instanssia käytetään.

`tools/list` sivutetaan `cursor`-arvolla vain, kun tunniste näkee yli 200 työkalua. Lista ei ilmoita muutoksista (`listChanged` on `false`): moduulin asennuksen jälkeen aloita uusi istunto.

### Kokeile curlilla

```bash
export OBOP_URL=https://YOUR-INSTANCE
export OBOP_TOKEN=obop_...
curl -si -X POST "$OBOP_URL/mcp" \
  -H "Authorization: Bearer $OBOP_TOKEN" \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

Kopioi vastauksen `mcp-session-id`-otsake, lähetä sitten `{"jsonrpc":"2.0","method":"notifications/initialized"}` (vastaus `202`) ja sen jälkeen `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`, kummankin mukana `-H "Mcp-Session-Id: <id>"`.

### Etäasiakkaiden asetukset

Korvaa `https://YOUR-INSTANCE/mcp` ja säilytä tunniste ympäristömuuttujassa tai salaisuuksien hallinnassa. Alla olevat muodot otettiin kunkin asiakkaan dokumentaatiosta 17.10.2026. Ajoimme obopia vain MCP Inspectorin komentoriviasiakkaalla (`vp dlx @modelcontextprotocol/inspector --cli URL --transport http --header "Authorization: Bearer ..." --method tools/list`) ja omilla Playwright-testeillämme. Emme käynnistäneet muita asiakkaita, joten jokainen esimerkki on **dokumentoitu, ei kokeiltu**.

#### Claude Code

```bash
claude mcp add --transport http obop https://YOUR-INSTANCE/mcp \
  --header "Authorization: Bearer $OBOP_TOKEN"
```

Lisää `--scope project`, jos haluat kirjoittaa jaetun `.mcp.json`-tiedoston, tai `--scope user`, jos haluat käyttää asetusta kaikissa projekteissa. Jaetussa tiedostossa lue tunniste ympäristöstä, jotta sitä ei viedä versionhallintaan:

```json
{
  "mcpServers": {
    "obop": {
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${OBOP_TOKEN}" }
    }
  }
}
```

Jos palvelin hylkää otsakkeen, Claude Code ilmoittaa epäonnistuneesta yhteydestä sen sijaan, että aloittaisi OAuthin. Tarkista komennolla `/mcp`.

#### Claude Desktop ja claude.ai

Sovellukset lisäävät etäpalvelimet _custom connector_ -liittiminä (Settings, Connectors, Add custom connector) ja tunnistavat ne OAuthilla. Syötä `https://YOUR-INSTANCE/mcp` ja seuraa yllä kuvattua selainkirjautumista; tunnistetta ei liitetä. claude.ai:ta varten instanssin on oltava tavoitettavissa internetistä `https`-yhteydellä, koska pyynnöt tekevät Anthropicin palvelimet. Stdio-asetus [alla](#claude-desktop-stdio) toimii Claude Desktopissa paikallisella ohjelmalla. Yhteisön `mcp-remote`-kaltainen silta voi lisätä otsakkeen etäpalvelimelle asiakkaille, jotka puhuvat vain stdiota; emme ole testanneet sitä obopin kanssa.

#### Cursor

`.cursor/mcp.json` (projekti) tai `~/.cursor/mcp.json` (kaikki projektit):

```json
{
  "mcpServers": {
    "obop": {
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### VS Code

`.vscode/mcp.json`. Ylätason avain on `servers`, ja syöte kysyy tunnisteen, jotta sitä ei kirjoiteta levylle:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "obop-token",
      "description": "obop agent token",
      "password": true
    }
  ],
  "servers": {
    "obop": {
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${input:obop-token}" }
    }
  }
}
```

VS Coden dokumentaatiossa ei ollut yhtä täydellistä esimerkkiä, jossa olisi sekä otsakkeet että syöte, joten tämä esimerkki on koottu sen kenttätaulukoista.

#### Windsurf

Windsurfin dokumentaatio on nyt Devin Desktop -sivustolla. Se mainitsee polut `~/.config/devin/mcp_config.json` (macOS ja Linux) ja `%APPDATA%\devin\mcp_config.json` (Windows); vanhemmat Windsurf-asennukset käyttivät polkua `~/.codeium/windsurf/mcp_config.json`. Tarkista, kumpaa oma versiosi lukee. Etäpalvelimet käyttävät kenttiä `serverUrl` (tai `url`) ja `headers`, ja `${env:NAME}` laajennetaan:

```json
{
  "mcpServers": {
    "obop": {
      "serverUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### Codex CLI

Tiedostossa `~/.codex/config.toml`. `bearer_token_env_var` nimeää ympäristömuuttujan, jossa tunniste on; Codex lähettää sen `Authorization`-Bearer-tunnisteena:

```toml
[mcp_servers.obop]
url = "https://YOUR-INSTANCE/mcp"
bearer_token_env_var = "OBOP_TOKEN"
```

`codex mcp add` dokumentoi `--url`-valitsimen vain OAuth-vaihtoehtojen kanssa, joten käytä Bearer-tunnisteelle tiedostoa.

#### Gemini CLI

Lukemamme dokumentaation mukaan Gemini CLI korvattiin Antigravity CLI:llä 18.6.2026; tarkista käyttämäsi työkalun sivu. Gemini CLI:n muoto on `httpUrl` Streamable HTTP -siirrolle (`url` on vanhemmalle SSE-siirrolle) tiedostossa `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "obop": {
      "httpUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer your-agent-token" }
    }
  }
}
```

tai `gemini mcp add --transport http --header "Authorization: Bearer $OBOP_TOKEN" obop https://YOUR-INSTANCE/mcp`. Emme voineet varmistaa, että tämä tiedosto laajentaa ympäristömuuttujat `headers`-kentässä, joten tunniste on kirjoitettu tähän näkyviin.

### Löydettävyys

Instanssi julkaisee nämä ilman tunnistetta:

| URL                                       | Mitä                                                                                                                                                                                        |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/.well-known/oauth-protected-resource`   | OAuth-suojatun resurssin metatiedot (RFC 9728) osoitteelle `/mcp`, nimeävät valtuutuspalvelimen. Myös osoitteessa `/.well-known/oauth-protected-resource/mcp`, johon `401`-vastaus viittaa. |
| `/.well-known/oauth-authorization-server` | OAuth-valtuutuspalvelimen metatiedot (RFC 8414): päätepisteet, vain PKCE `S256`, vain julkiset asiakkaat.                                                                                   |
| `/.well-known/mcp/server-card.json`       | MCP-palvelinkortti: nimi, versio, `streamable-http`-päätepiste, protokollaversiot, `Authorization`-otsake. Myös osoitteessa `/mcp/server-card`.                                             |
| `/.well-known/ai-catalog.json`            | Yhden merkinnän AI Catalog, joka osoittaa palvelinkorttiin.                                                                                                                                 |
| `/api/openapi.json`, `/api/meta`          | OpenAPI-dokumentissa on `x-mcp`-kenttä, jossa ovat päätepiste, siirto ja protokollaversiot; `GET /api/meta` sisältää `Link`-otsakkeen arvolla `rel="mcp"`.                                  |

Palvelinkortti on MCP-määrittelyn laajennus, jota vielä kehitetään (SEP-2127, pidetään kehitysvaiheen repositoriossa), joten sen muoto voi muuttua; tiedosto kertoo tämän `$comment`-kentässään. Siinä on myös pelkät `serverInfo`-, `transport`-, `endpoint`- ja `capabilities`-kentät aiemmasta luonnoksesta niitä etsiviä skannereita varten.

## MCP stdio-yhteydellä (paikallinen ohjelma)

Käytä tätä, kun asiakas voi vain käynnistää paikallisen ohjelman tai kun ajat obopia samalla koneella. Yllä oleva etäpalvelin ei tarvitse mitään tästä.

### Näin se toimii

`obop mcp` on MCP-palvelin, joka puhuu JSON-RPC:tä vakiosyötteen ja -tulosteen kautta. MCP-asiakas käynnistää sen aliprosessina. Palvelin:

- avaa instanssin tietokannan itse, joten sen on toimittava koneella, jolta tietokantaan (SQLite-tiedosto tai PostgreSQL-osoite) on yhteys;
- tunnistaa agentin agenttitunnisteesta;
- listaa samat työkalut samoilla skeemoilla kuin etäpalvelin (molemmat siirrot käyttävät samaa koodipolkua): yhden työkalun jokaista komentoa kohti, jota tunniste saa käyttää, nimellä `<model>__<command>`. Kaksoisalaviiva on samassa kohdassa kuin vinoviiva REST-polussa;
- vastaa `tools/call`-kutsuihin saman jakelijan kautta kuin REST, joten maskaus ja hyväksynnät ovat voimassa. Maskitunnisteet pysyvät vakaina niin kauan kuin prosessi on käynnissä.

### Ennen aloitusta

1. Tarvitset käynnissä olevan instanssin tietokannan ja `obop`-ohjelman koneella, jolla MCP-asiakas ajetaan (tai sen absoluuttisen polun).
2. Tarvitset [agenttitunnisteen](/docs/agents/tokens/), jossa on haluamasi rajaukset.
3. Tarvitset tietokannan osoitteen muodossa `OBOP_DATABASE_URL` tai `--db`. Oletus on `sqlite://obop.db` nykyisessä hakemistossa, joten anna absoluuttinen polku: `sqlite:///home/me/obop.db`.

Jokaisen asiakkaan ajama komento on:

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

Se lukee `OBOP_TOKEN`- ja `OBOP_DATABASE_URL`-muuttujat ympäristöstä, jos valitsimia ei ole annettu. Suosi ympäristöä: silloin tunniste ei näy prosessiluettelossa.

Kokeile ensin käsin. Tämä lähettää `initialize`- ja `tools/list`-pyynnön ja tulostaa vastaukset:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

Palvelin vastaa siihen versioon, jota asiakas pyytää, jos se on jokin versioista `2025-11-25`, `2025-06-18` ja `2025-03-26`, ja tukee kutsuja `initialize`, `ping`, `tools/list` ja `tools/call`. Työkalun argumentit ovat samaa JSONia kuin [REST-pyynnön runko](/docs/reference/rest-api/).

Alla olevissa esimerkeissä käytetään polkua `/usr/local/bin/obop` ja paikkamerkkinä toimivaa tunnistetta. Korvaa molemmat. Muodot tarkistettiin kunkin asiakkaan dokumentaatiosta 16.10.2026; kohdat, joita ei voitu varmistaa, on merkitty.

### Claude Desktop (stdio)

Avaa _Settings_, sitten _Developer_ ja _Edit Config_. Tiedosto on macOS:ssa `~/Library/Application Support/Claude/claude_desktop_config.json` ja Windowsissa `%APPDATA%\Claude\claude_desktop_config.json`. Lisää:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "obop_your_agent_token",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Sulje Claude Desktop ja käynnistä se uudelleen. Claude pyytää hyväksyntääsi ennen työkalun käyttöä.

### Claude Code (stdio)

Käytä komentoriviä. Kaikki merkkien `--` jälkeen on palvelimen komento:

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Laita viimeisen `--env`-valitsimen ja palvelimen nimen väliin vähintään yksi muu valitsin (tässä `--transport stdio`). Lisää `--scope project`, jos haluat kirjoittaa jaetun `.mcp.json`-tiedoston, tai `--scope user`, jos haluat käyttää asetusta kaikissa projekteissa. Projektitiedosto näyttää tältä; muoto `${...}` lukee arvon ympäristöstäsi, joten tunnistetta ei viedä versionhallintaan:

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

Tarkista asetus komennolla `claude mcp get obop` tai Claude Codessa komennolla `/mcp`.

### Cursor (stdio)

Luo tiedosto `.cursor/mcp.json` projektiin tai `~/.cursor/mcp.json` kaikkia projekteja varten:

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Cursorin omissa esimerkeissä `type` on jätetty pois stdio-palvelimilta, mutta sen kenttätaulukossa se mainitaan. Muoto `${env:NAME}` lukee arvon ympäristöstä, jossa Cursor on käynnistetty.

### VS Code (stdio)

Luo tiedosto `.vscode/mcp.json`. VS Coden ylätason avain on `servers`, ei `mcpServers`, ja se voi kysyä tunnisteen, jolloin sitä ei koskaan kirjoiteta levylle:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "obop-token",
      "description": "obop agent token",
      "password": true
    }
  ],
  "servers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${input:obop-token}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

VS Coden ohjattu _MCP: Add Server_ -komento voi ehdottaa sen sijaan siirrettävää `.mcp.json`-tiedostoa, jossa on `mcpServers`-muoto; dokumentaation mukaan molemmat hyväksytään.

### Windsurf (stdio)

Windsurfin dokumentaatio on siirtynyt Devin Desktop -sivustolle, ja siinä tiedosto on nimeltään `mcp_config.json`. Windsurfin nykyistä tiedostosijaintia ei voitu varmistaa. Windsurf on aiemmin käyttänyt polkua `~/.codeium/windsurf/mcp_config.json`; Devin Desktop -sivulla annetaan polut `~/.config/devin/mcp_config.json` (macOS ja Linux) ja `%APPDATA%\devin\mcp_config.json` (Windows). Tarkista sijainti omasta versiostasi asetuksista. Muoto on sama `mcpServers`-rakenne:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

### Codex CLI (stdio)

Lisää taulu tiedostoon `~/.codex/config.toml` (tai luotetun projektin tiedostoon `.codex/config.toml`) tai aja `codex mcp add`:

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` välittää `OBOP_TOKEN`-muuttujan komentotulkistasi sen sijaan, että se kirjoitettaisiin tiedostoon.

### Gemini CLI (stdio)

Lisää tiedostoon `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "$OBOP_TOKEN",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

Gemini CLI:n dokumentaation mukaan se korvattiin toisella työkalulla vuonna 2026, joten tarkista oman versiosi ohjeet sieltä. Emme ole ajaneet obopia sen kanssa.

### Mikä tahansa muu MCP-asiakas

obop on tavallinen stdio-palvelin. Asiakkaan tiedosto voi näyttää miltä tahansa, mutta se tarvitsee `command`-arvon (`obop`), `args`-arvon (`["mcp"]`) ja kaksi ympäristömuuttujaa. Jos asiakas tukee vain etäpalvelimia, käytä yllä olevaa etäpalvelinta.

## Mitä agentti näkee

Pyydä yhdistämisen jälkeen agenttia listaamaan työkalunsa. Näkyviin pitäisi tulla vain ne komennot, jotka sen rajaukset sallivat, ja hyväksyntää vaativista on merkintä. Kokeile ensin lukua, esimerkiksi _"List the three newest leads"_, ja katso sitten [Hyväksyntäkierros](/docs/agents/approvals/).

Jos asiakas ei näytä työkaluja tai näyttää virheen, siirry kohtaan [Vianmääritys](/docs/agents/troubleshooting/).

Jokaisella mallilla, jonka agentti saa luoda, on myös työkalu `<model>__bulk_create`: enintään 1 000 riviä yhdessä kutsussa (`rows` tai `csv`-teksti), oletuksena kaikki tai ei mitään, `mode: "partial"` säilyttää hyvät rivit ja luettelee huonot, `dry_run: true` tarkistaa ensin ja `idempotency_key` tekee uusinnasta turvallisen. Peitto, laajuus ja hyväksyntä ovat samat kuin `create`-komennolla.

Agentti, jonka omistaja on ylläpitäjä, näkee myös `user_account`-työkalut (`user_account__list`, `__get`, `__add`, `__set_groups`, `__offboard`, `__disable`, `__enable`). Tiliä muuttavat palauttavat aina `approval_required`: ylläpitäjä hyväksyy suunnitelman, ja se suoritetaan tämän ylläpitäjän nykyisenä tilinä. `__add` vaatii ihmisen valitseman salasanan; palvelin ei koskaan anna agentille luotua salasanaa.

## Varmennettu ja varmentamatta

| Kohta                                                                                              | Tila                                                                                   |
| -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `obop mcp` -kättely, `tools/list`, `tools/call`, hyväksynnän tulos                                 | Ajettu oikeaa paikallista instanssia vasten.                                           |
| Etä-`/mcp`: kättely, työkalut, maskaus, hyväksyntäkierros, istunnot, 401, origin-tarkistus, rajat  | Automaattiset testit (Rust HTTP:n yli, Playwright) oikeaa instanssia vasten.           |
| Etä-`/mcp` oikealla MCP-asiakkaalla                                                                | MCP Inspectorin komentoriviasiakas 2.11.0: `initialize` ja `tools/list`, `tools/call`. |
| Virallinen MCP-yhteensopivuustestisarja                                                            | Ei mielekäs: se vaatii palvelimen ilman todennusta ja omat testityökalunsa.            |
| Claude Coden, Cursorin, VS Coden, Windsurfin, Codex CLI:n ja Gemini CLI:n etäasetukset             | Otettu kunkin asiakkaan dokumentaatiosta; asiakkaita ei käynnistetty.                  |
| Stdio-asetusmuodot                                                                                 | Otettu kunkin asiakkaan dokumentaatiosta. Emme käynnistäneet jokaista asiakasta.       |
| Windsurfin tiedostosijainti                                                                        | Varmistamatta, katso yllä.                                                             |
| Selainkirjautuminen (OAuth): rekisteröinti, PKCE, suostumus, tunnisteet, uudelleenkäyttö, peruutus | Automaattiset testit (Rust ja Playwright oikeassa selaimessa) omalla asiakkaallamme.   |
| Claude Desktopin ja claude.ain liittimet, Claude Coden kirjautuminen                               | **Ei kokeiltu** oikeilla sovelluksilla; rakennettu määrittelyn mukaan.                 |
