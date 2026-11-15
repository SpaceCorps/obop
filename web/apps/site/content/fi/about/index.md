---
title: "Tietoa"
description: "Tietoa obopista: SpaceCorpsin tekemä avoimen lähdekoodin yritysalusta ihmisille ja tekoälyagenteille, jonka lähtökohtia ovat tietosuoja, käyttöoikeudet ja avoin ylläpito."
lead: "obop on avoimen lähdekoodin käyttöjärjestelmä yrityksille: yksi alusta, yksi tietomalli ja rehellinen kuvaus siitä, mikä on valmista."
eyebrow: "Tietoa"
---

## Mitä rakennamme

Modulaarinen yrityssovellusten kokonaisuus suurten kaikenkattavien toiminnanohjausjärjestelmien hengessä, mutta avoin, siirrettävä ja alusta asti valmis tekoälyagenteille. Sen sisäänrakennettuihin sovelluksiin kuuluvat yhteystiedot, tuotteet, CRM, myynti, laskutus ja liittimet (ne kuusi, joilla on oma sovellussivu), ja siinä on 24 mukana toimitettua pakettia (kirjanpito, varasto, valmistus, henkilöstöhallinto, projektit, kassajärjestelmä, verkkokauppa, verkkosivusto, sähköpostimarkkinointi, asiakaspalvelu, asiakirjat ja muita), jotka ovat vielä beta- tai kokeiluvaiheessa.

obop on puhtaalta pöydältä tehty uusi toteutus ideoista, jotka yrityssovellukset ovat todistaneet toimiviksi vuosikymmenten aikana. Se ei kopioi kenenkään koodia.

## Periaatteet

- **Yksi yritys, yksi alusta.** Yksi tietomalli kaikille sovelluksille, joten mitään ei tarvitse liimata yhteen.
- **Agentit ovat ensiluokkaisia, ja rajattuja.** Rajatut tunnisteet, maskatut henkilötiedot ja ihmisen hyväksyntä jokaiselle poistolle sekä vaiheille, jotka moduuli merkitsee hyväksyntää vaativiksi.
- **Avoin ja siirrettävä.** Lähdekoodi on julkinen, alusta toimii PostgreSQL:llä, ja yrityksen tiedot kuuluvat sille itselleen.
- **Rehellinen.** Kerromme, mikä on tehty ja mikä ei. Ei keksittyjä asiakkaita, lainauksia tai lukuja: tällä sivustolla ei ole niitä, koska näytettävää ei vielä ole.
- **Selkeä kieli.** Dokumentaatio ja käyttöliittymä sanovat sen, mitä tarkoittavat.

## Kuka sitä tekee

obopin tekee [SpaceCorps](https://github.com/SpaceCorps). Koodi, päätökset ja tiekartta ovat [repositoriossa](https://github.com/SpaceCorps/obop).

## Teknologia

Taustajärjestelmä on tehty Rustilla. Verkkosovellukset käyttävät Vite+:aa ja Reactia. Tämä verkkosivusto on VitePressillä Markdownista ja yhteisestä suunnittelujärjestelmästä generoitu staattinen sivusto. Käyttöliittymä ja tärkeimmät sivut ovat saatavilla seitsemällä kielellä (konekäännettyinä); muut sivut näytetään englanniksi.

## Mihin seuraavaksi

- [Alusta](/platform/): näin se toimii.
- [Tiekartta](/roadmap/): mikä on valmista ja mikä ei.
- [Yhteisö](/about/community/) ja [Tila](/about/status/).
- [Yhteystiedot](/contact/).
