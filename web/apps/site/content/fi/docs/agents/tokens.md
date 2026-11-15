---
title: "Luo agenttitunniste"
description: "Luo rajattu obop-agenttitunniste hallintanäkymässä, REST-rajapinnan kautta (POST /api/auth/agent-token) tai CLI:llä (obop agent-token). Tunniste näytetään kerran, ja se voi vain kaventaa sitä, mitä sen omistaja saa tehdä."
lead: "Agenttitunniste on Bearer-tunniste, joka kuuluu henkilölle ja sisältää listan käyttöoikeusryhmiä. Luo yksi jokaista agenttia ja tehtävää varten."
---

## Mikä tunniste on

- Se kuuluu sen luoneelle henkilölle (_omistajalle_) ja toimii osajoukolla tämän ryhmistä.
- Se näytetään **kerran**. obop tallentaa vain tiivisteen, joten kadonnutta tunnistetta ei voi palauttaa; luo uusi.
- Se alkaa merkkijonolla `obop_`. Kohtele sitä kuin salasanaa.
- Vain henkilö voi luoda tunnisteita. Agentti ei voi luoda toista tunnistetta.
- Se ei vanhene. Sen omistaja voi mitätöidä sen milloin tahansa (katso [Luettele ja mitätöi](#luettele-ja-mitatoi)). Se ei myöskään toimi niin kauan kuin omistajan tili on poistettu käytöstä, ja se lakkaa toimimasta pysyvästi, kun omistajan käyttö lopetetaan pysyvästi (offboarding).

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

## Hallintanäkymässä

Avaa _Mukauta, Tietoturva_ ja siirry kohtaan **Agenttitunnukset**. Kirjoita nimi (ohjelma tai agentti, joka käyttää tunnistetta), rastita ryhmät ja valitse **Luo agenttitunnus**. Tunniste näytetään kerran, ja mukana on **Kopioi**-painike; valitse **Valmis**, kun olet tallentanut sen. Osiossa näkyvät myös luomasi tunnisteet, ja jokaisella on **Peruuta**-painike.

## REST-rajapinnan kautta

Kirjaudu sisään henkilönä ja pyydä sitten tunniste. Tämä vaatii käynnissä olevan instanssin; esimerkeissä käytetään paikallista instanssia portissa 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

Vastaus on `{"token":"obop_..."}`: istuntotunniste, joka on voimassa 12 tuntia. Jos tilillä on kaksivaiheinen kirjautuminen käytössä, vastaus on sen sijaan `{"mfa_required":true,"mfa_token":"..."}`; lähetä `{"mfa_token":"...","code":"123456"}` todennussovelluksesi koodilla osoitteeseen `POST /api/auth/login/mfa`, niin saat istuntotunnisteen. Käytä istuntotunnistetta Bearer-tunnisteena agenttitunnisteen luomiseen:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

Kenttä `scopes` on pakollinen, eikä se saa olla tyhjä. Siinä nimetään ryhmiä, jotka on merkitty omalle tilillesi. Ylläpitäjä (henkilö, jolla on `platform.admin`) voi nimetä minkä tahansa instanssin ryhmän, koska se ryhmä sallii jo kaikki. Tunnisteessa on silti vain ne ryhmät, jotka nimeät: `*` ei ole ryhmä ja se hylätään, ja ryhmä, jota mikään asennettu moduuli ei määrittele, hylätään myös. Kentän `label` pituus on enintään 80 merkkiä, ja sen on mahduttava yhdelle riville. Ohjaustason luomassa instanssissa käytä sen osoitetta osoitteen `127.0.0.1:8080` sijaan ja kirjaudu sisään rekisteröitymisessä käyttämälläsi sähköpostiosoitteella käyttäjätunnuksena.

## CLI:llä

Jos ajat obopia itse, voit luoda tunnisteen suoraan komentoriviltä. Komento käyttää tietokantaa suoraan, joten aja se paikassa, josta tietokantaan on yhteys:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Lisää `--db postgres://...` (tai aseta `OBOP_DATABASE_URL`), jos instanssi ei käytä oletusta `sqlite://obop.db`. Rajaukset noudattavat samaa sääntöä kuin REST-rajapinnan kautta, nimeämäsi käyttäjän osalta. Komentorivillä ei voi vielä luetella eikä mitätöidä tunnisteita.

## Luettele ja mitätöi

Näet ja mitätöit omat agenttitunnisteesi kohdassa _Mukauta, Tietoturva, Agenttitunnukset_ tai REST-rajapinnan kautta istuntotunnisteellasi:

```bash
curl -s http://127.0.0.1:8080/api/auth/agent-tokens \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

```json
{
  "tokens": [
    {
      "id": "5f0c...",
      "label": "triage-bot",
      "scopes": ["base.user", "crm.user"],
      "created_at": 1794650000
    }
  ],
  "delegable": ["base.user", "crm.user"]
}
```

`tokens` ei koskaan sisällä tunnistetta, vain sen, mikä kukin on nimeltään ja mitä se saa tehdä. `delegable` on luettelo ryhmistä, jotka voit antaa uudelle tunnisteelle. Mitätöi tunniste lähettämällä POST sen `id`:n mukaiseen osoitteeseen:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-tokens/5f0c.../revoke \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

Tunniste lakkaa toimimasta heti. Vain omistaja voi tehdä tämän: ylläpitäjä ei voi luetella tai mitätöidä toisen henkilön tunnisteita yksi kerrallaan (tilin poistaminen käytöstä tai offboarding lopettaa ne kaikki). Tunnisteen luominen ja mitätöinti kirjataan molemmat tietoturvalokiin, ilman itse tunnistetta.

## Käytä tunnistetta MCP:ssä

Sama tunniste yhdistää MCP-asiakkaan instanssin etä-MCP-palvelimeen `https://YOUR-INSTANCE/mcp`: lähetä se muodossa `Authorization: Bearer obop_...` (asiakkailla on sitä varten `headers`-kenttä tai Bearer-tunnisteen muuttuja). CLI:llä tai REST-rajapinnan kautta luotu tunniste toimii molempiin; mitään MCP-kohtaista ei tarvitse luoda. Pidä mielessä nämä asiat:

- Luo erillinen tunniste jokaista asiakasta varten ja anna sille vain ne rajaukset, joita asiakas tarvitsee. MCP-työkalulista on täsmälleen ne komennot, jotka rajaukset sallivat.
- Myös sisäänkirjautumisen istuntotunniste toimii, mutta se vanhenee 12 tunnissa (agenttitunniste ei vanhene), ja se toimii sinun nimissäsi: henkilötietoja ei maskata, ja hyväksyntää vaativat komennot suoritetaan ilman hyväksyntää. Käytä agentille agenttitunnistetta.
- Tunniste on lähetettävä jokaisessa pyynnössä, ei vain ensimmäisessä; asiakkaat tekevät tämän, kun tunniste on niiden asetuksissa.
- Asiakkaat, jotka tukevat selainkirjautumista (OAuth), eivät tarvitse liitettyä tunnistetta: ne yhdistävät kirjautumalla ja saavat tunnisteen, joka on voimassa tunnin ja toimii vain osoitteessa `/mcp`. Emme ole vielä kokeilleet tätä Claude Desktopilla, claude.ai:lla tai Claude Codella; katso [Yhdistä MCP:n kautta](/docs/agents/mcp/).

## Säilytä se turvallisesti

- Tallenna se ympäristömuuttujaan tai salaisuuksien hallintaan, älä repositorioon.
- Käytä mieluiten yhtä tunnistetta agenttia ja tehtävää kohti, kussakin vain ne rajaukset, joita kyseinen agentti tarvitsee.
- Asiakasohjelman asetustiedostossa oleva tunniste on luettavissa kaikilta, jotka voivat lukea tiedoston.

## Virheet

| Vastaus                                                                            | Merkitys                                                                                                                  |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                          | `scopes` oli tyhjä.                                                                                                       |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}`     | Pyysit ryhmää, jota ei ole tililläsi, etkä ole ylläpitäjä.                                                                |
| `400 {"error":"nope.user is not a registered permission group, ..."}`              | Ylläpitäjä pyysi ryhmää, jota mikään asennettu moduuli ei määrittele.                                                     |
| `400 {"error":"only a human user can issue agent tokens"}`                         | Käytit agenttitunnistetta pyytääksesi toista.                                                                             |
| `401`                                                                              | Istuntotunniste puuttuu, on väärä tai vanhentunut.                                                                        |
| `403 {"error":"two-factor authentication must be set up before you can continue"}` | Tilisi on ensin otettava kaksivaiheinen kirjautuminen käyttöön; siihen asti tällä istunnolla voi vain ottaa sen käyttöön. |

Seuraavaksi: [yhdistä MCP:n kautta](/docs/agents/mcp/) tai [käytä REST-rajapintaa](/docs/agents/rest/).
