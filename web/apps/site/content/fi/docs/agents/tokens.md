---
title: "Luo agenttitunniste"
description: "Luo rajattu obop-agenttitunniste REST-rajapinnan (POST /api/auth/agent-token) tai CLI:n (obop agent-token) kautta. Tunniste näytetään kerran, ja se voi vain kaventaa omistajansa käyttöoikeusryhmiä."
lead: "Agenttitunniste on Bearer-tunniste, joka kuuluu henkilölle ja sisältää listan käyttöoikeusryhmiä. Luo yksi jokaista agenttia ja tehtävää varten."
---

## Mikä tunniste on

- Se kuuluu sen luoneelle henkilölle (_omistajalle_) ja toimii osajoukolla tämän ryhmistä.
- Se näytetään **kerran**. obop tallentaa vain tiivisteen, joten kadonnutta tunnistetta ei voi palauttaa; luo uusi.
- Se alkaa merkkijonolla `obop_`. Kohtele sitä kuin salasanaa.
- Vain henkilö voi luoda tunnisteita. Agentti ei voi luoda toista tunnistetta.
- Se ei vanhene, ja se lakkaa toimimasta, kun omistaja poistetaan. Yksittäisen tunnisteen mitätöintikomentoa ei ole vielä (katso [Turvallisuus](/docs/agents/safety/)).

## Valitse rajaukset

Rajaukset ovat [käyttöoikeusryhmiä](/docs/concepts/permission-groups/). Mieti, mitä agentin pitää tehdä, ja anna sille vain nuo ryhmät. Sisäänrakennettujen sovellusten yleisiä ryhmiä:

| Ryhmä               | Agentti voi                                                                          |
| ------------------- | ------------------------------------------------------------------------------------ |
| `base.user`         | lukea ja kirjoittaa Contacts-sovelluksen tietoja (henkilötiedot pysyvät maskattuina) |
| `crm.user`          | lukea ja kirjoittaa liidejä ja siirtää niitä myyntiputkessa                          |
| `sales.user`        | lukea ja kirjoittaa tarjouksia ja tilauksia                                          |
| `invoicing.user`    | lukea ja kirjoittaa laskuja sekä peruuttaa luonnoksia                                |
| `invoicing.manager` | ehdottaa laskun kirjaamista (ihminen hyväksyy silti)                                 |
| `product.user`      | lukea tuotteita ja hintoja                                                           |

Asennetut paketit tuovat omat ryhmänsä, esimerkiksi `helpdesk.agent`. [Sovellussivuilla](/apps/) luetellaan kunkin sovelluksen ryhmät.

## REST-rajapinnan kautta

Kirjaudu sisään henkilönä ja pyydä sitten tunniste. Tämä vaatii käynnissä olevan instanssin; esimerkeissä käytetään paikallista instanssia portissa 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

Vastaus on `{"token":"obop_..."}`: istuntotunniste, joka on voimassa 12 tuntia. Käytä sitä Bearer-tunnisteena agenttitunnisteen luomiseen:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

Kenttä `scopes` on pakollinen, ja siinä on oltava vähintään yksi ryhmä, joka sinulla on. Kenttä `label` on vapaata tekstiä. Isännöidyssä instanssissa käytä sen osoitetta osoitteen `127.0.0.1:8080` sijaan ja kirjaudu sisään rekisteröitymisessä käyttämälläsi sähköpostiosoitteella käyttäjätunnuksena.

## CLI:llä

Jos ajat obopia itse, voit luoda tunnisteen suoraan komentoriviltä. Komento käyttää tietokantaa suoraan, joten aja se paikassa, josta tietokantaan on yhteys:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Lisää `--db postgres://...` (tai aseta `OBOP_DATABASE_URL`), jos instanssi ei käytä oletusta `sqlite://obop.db`.

## Käytä tunnistetta MCP:ssä

Sama tunniste yhdistää MCP-asiakkaan instanssin etä-MCP-palvelimeen `https://YOUR-INSTANCE/mcp`: lähetä se muodossa `Authorization: Bearer obop_...` (asiakkailla on sitä varten `headers`-kenttä tai Bearer-tunnisteen muuttuja). CLI:llä tai REST-rajapinnan kautta luotu tunniste toimii molempiin; mitään MCP-kohtaista ei tarvitse luoda. Pidä mielessä nämä asiat:

- Luo erillinen tunniste jokaista asiakasta varten ja anna sille vain ne rajaukset, joita asiakas tarvitsee. MCP-työkalulista on täsmälleen ne komennot, jotka rajaukset sallivat.
- Myös sisäänkirjautumisen istuntotunniste toimii, mutta se vanhenee 12 tunnissa; agenttitunniste ei vanhene.
- Tunniste on lähetettävä jokaisessa pyynnössä, ei vain ensimmäisessä; asiakkaat tekevät tämän, kun tunniste on niiden asetuksissa.
- Asiakkaat, jotka tarjoavat selainkirjautumisen (Claude Desktop, claude.ai, Claude Code), eivät tarvitse liitettyä tunnistetta: ne yhdistävät kirjautumalla ja saavat tämänlaisen lyhytikäisen tunnisteen. Katso [Yhdistä MCP:n kautta](/docs/agents/mcp/).

## Säilytä se turvallisesti

- Tallenna se ympäristömuuttujaan tai salaisuuksien hallintaan, älä repositorioon.
- Käytä mieluiten yhtä tunnistetta agenttia ja tehtävää kohti, jotta ne voi erottaa toisistaan ja yhden voi poistaa muihin koskematta.
- Asiakasohjelman asetustiedostossa oleva tunniste on luettavissa kaikilta, jotka voivat lukea tiedoston.

## Virheet

| Vastaus                                                                        | Merkitys                                           |
| ------------------------------------------------------------------------------ | -------------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                      | `scopes` oli tyhjä.                                |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}` | Pyysit ryhmää, jota sinulla ei ole.                |
| `400 {"error":"only a human user can issue agent tokens"}`                     | Käytit agenttitunnistetta pyytääksesi toista.      |
| `401`                                                                          | Istuntotunniste puuttuu, on väärä tai vanhentunut. |

Seuraavaksi: [yhdistä MCP:n kautta](/docs/agents/mcp/) tai [käytä REST-rajapintaa](/docs/agents/rest/).
