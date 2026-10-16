---
title: "Agentit"
description: "Näin tekoälyagentit otetaan käyttöön obopissa: agenttimalli, rajatut tunnisteet, maskatut henkilötiedot ja hyväksynnät, jotka pitävät ihmisen vastuussa kaikesta peruuttamattomasta."
lead: "obop kohtelee tekoälyagentteja ensiluokkaisina käyttäjinä, joita rajoitetaan tarkoituksella. Tällä sivulla kuvataan malli; seuraavilla sivuilla näytetään, miten agentti otetaan käyttöön."
---

## Agenttimalli

**Agentti** on mikä tahansa taho, joka toimii obopissa agenttitunnisteella: MCP-asiakas, kuten Claude Desktop tai Cursor, REST-rajapintaa kutsuva skripti tai komentorivi. Agentit työskentelevät samojen tietueiden parissa ja samojen komentojen kautta kuin ihmiset. Neljä sääntöä tekee tästä turvallista.

1. **Rajattu.** Agenttitunnisteeseen kuuluu lista käyttöoikeusryhmiä (sen _rajaukset_). Tunniste voi vain kaventaa omistajansa oikeuksia, ei koskaan laajentaa niitä. Ryhmän pyytäminen, jota omistajalla ei ole, evätään.
2. **Maskattu.** Henkilötiedot palautetaan pysyvinä paikkamerkkeinä, kuten `EMAIL_1`. Talous-, henkilöstö- ja erityisryhmien tietokentät pidätetään. Agentti voi silti päätellä tietueista ja välittää paikkamerkkejä takaisin, mutta se ei koskaan näe oikeita arvoja. Katso [Maskauksen paikkamerkit](/docs/agents/masking/).
3. **Ehdota, älä toteuta.** Agentti voi suorittaa tavallisia toimintoja rajaustensa puitteissa, mutta kaikesta peruuttamattomasta (laskun kirjaaminen, maksaminen, hyvittäminen, lähettäminen, julkaiseminen, poistaminen) se voi vain _ehdottaa_. Palvelin palauttaa suunnitelman tunnuksen eikä muuta mitään. Ihminen hyväksyy. Katso [Hyväksyntäkierros](/docs/agents/approvals/).
4. **Sama ovi.** REST, MCP ja CLI kulkevat saman jakelijan kautta, joten agentilla on täsmälleen samat käyttöoikeudet, maskaus ja hyväksynnät riippumatta siitä, miten se yhdistää.

## Kolme tapaa yhdistää

| Rajapinta            | Käytä, kun                                                                                                     | Sivu                                        |
| -------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| MCP stdio-yhteydellä | MCP-asiakkaan (Claude Desktop, Claude Code, Cursor, VS Code, Windsurf ja muut) pitää kutsua obopia työkaluina. | [Yhdistä MCP:n kautta](/docs/agents/mcp/)   |
| REST                 | Skripti tai palvelu kutsuu HTTP-päätepisteitä.                                                                 | [Käytä REST-rajapintaa](/docs/agents/rest/) |
| CLI                  | Komentorivi tai cron-ajo kutsuu komentoa `obop call`.                                                          | [Käytä CLI:tä](/docs/agents/cli/)           |

## Käyttöönotto viidessä vaiheessa

1. Päätä, mitä agentin pitää tehdä ja mitä ryhmiä se tarvitsee. Vähemmän on parempi.
2. [Luo agenttitunniste](/docs/agents/tokens/), jossa on vain nuo rajaukset.
3. Yhdistä agentti [MCP:n](/docs/agents/mcp/), [REST-rajapinnan](/docs/agents/rest/) tai [CLI:n](/docs/agents/cli/) kautta.
4. Kokeile vaaratonta lukua ja sitten hyväksyntää vaativaa muutosta, ja hyväksy se ihmisenä.
5. Lue [Turvallisuuden parhaat käytännöt](/docs/agents/safety/) ennen kuin annat tunnisteen millekään valvomattomalle.

## Mitä on tehty ja mitä ei

- Tehty: rajatut tunnisteet, maskaus ja palautus, hyväksyntäsuunnitelmat, MCP stdio-yhteydellä, REST ja CLI.
- Ei vielä tehty: HTTP-pohjainen (etä)MCP-siirto OAuthilla, joten isännöityihin instansseihin ei vielä pääse MCP:llä; näkymä agenttitunnisteiden luomiseen tai mitätöintiin; luettelo odottavista hyväksynnöistä käyttöliittymässä. Agenttitunnisteet eivät vanhene, eikä mitätöintikomentoa ole vielä. Nämä mainitaan uudelleen kohdissa, joissa niillä on merkitystä.

[Esimerkkityönkuluissa](/docs/agents/workflows/) on kehotteita ja kulkuja, joista voi lähteä liikkeelle, ja [Vianmäärityksestä](/docs/agents/troubleshooting/) apua, kun jokin ei toimi.
