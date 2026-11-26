---
title: "Verkkolaskutus Saksaan ja Ruotsiin: mitä rakennamme"
description: "Saksa tekee rakenteisesta verkkolaskusta pakollisen kaikille yrityksille vuodesta 2028, ja Ruotsin julkiset hankkijat ottavat niitä jo vastaan. Näin obop aikoo lukea, tarkistaa ja kirjoittaa niitä, ja tämä on olemassa jo nyt."
date: "2026-11-25"
---

Alkaen 2028-01-01 jokaisen Saksaan sijoittautuneen yrityksen on lähetettävä muille Saksassa toimiville yrityksille osoitetut laskunsa rakenteisina verkkolaskuina. PDF ei enää riitä. Tämä on tärkein syy siihen, että Saksa on obopin ensimmäinen maa, ja verkkolaskutus on työ, jonka olemme nyt aloittaneet. Ruotsi vaatii samaa eurooppalaista standardia julkisille hankkijoille lähetettäviltä laskuilta. Tämä kirjoitus kertoo, mitä laki vaatii, mitä obopissa on tänään ja mitä rakennamme.

## Mitä laki vaatii

Saksan laki (§ 14 UStG) määrittelee verkkolaskun laskuksi, joka laaditaan, lähetetään ja vastaanotetaan rakenteisessa sähköisessä muodossa. Muodon on noudatettava eurooppalaista verkkolaskustandardia EN 16931 tai oltava jokin muu osapuolten sopima muoto, jota voi edelleen käsitellä sähköisesti. Laskut on säilytettävä kahdeksan vuotta niin, että niiden alkuperä, sisältö ja luettavuus on turvattu (§ 14b UStG). obop säilyttää vastaanotetun verkkolaskun XML-tiedoston muuttamattomana. Velvoite tulee voimaan vaiheittain:

- **Alkaen 2025-01-01:** jokaisen yrityksen on pystyttävä vastaanottamaan verkkolaskuja.
- **Alkaen 2027-01-01:** yritysten, joiden liikevaihto edellisenä vuonna oli yli 800 000 EUR, on laadittava niitä.
- **Alkaen 2028-01-01:** jokaisen yrityksen on laadittava niitä.

Poikkeuksia ovat muun muassa enintään 250 EUR:n laskut (§ 33 UStDV) sekä vuoden 2027 loppuun asti vastaanottajan kanssa sovitut EDI-menettelyt (§ 27 Abs. 38 UStG).

Saksa hyväksyy useita muotoja. obop aikoo tukea niistä kolmea: XRechnung, ZUGFeRD 2.5.2 (sama kuin Factur-X 1.09.2, PDF, jonka sisällä on XML) ja Peppol BIS Billing 3.0. Näiden muotojen taustalla on kaksi XML-syntaksia, UBL ja CII, ja molemmat kuljettavat samaa EN 16931 -sisältöä.

## Ruotsi

Ruotsissa velvoite koskee julkiselle sektorille lähetettäviä laskuja. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) tuli voimaan 2019-04-01. Siitä päivästä alkaen aloitetuissa hankinnoissa toimittajan julkiselle hankkijalle lähettämien laskujen on oltava eurooppalaista standardia EN 16931 noudattavia sähköisiä laskuja (4 §), ja julkisten hankkijoiden on otettava vastaan ja käsiteltävä tällaisia laskuja (5 §). Toimittaja ja hankkija voivat sopia toisesta standardista. Laki ei nimeä muotoa. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), joka on vastannut asiaan liittyvistä määräyksistä 2026-07-01 alkaen, suosittelee Peppol BIS Billing 3:a, joka on EN 16931:n toteutus, ja [valtion virastojen on tuettava sitä](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Julkisten hankkijoiden on oltava rekisteröityinä Peppoliin laskujen vastaanottajina, ja niiden on pystyttävä ottamaan vastaan sekä UBL että CII.

## Mitä obopissa on tänään

obopissa on Laskutus-sovellus. Se pitää kirjaa asiakaslaskuista ja niiden riveistä, laskee loppusumman ja antaa jokaiselle kirjatulle laskulle seuraavan numeron aukottomasta vuosittaisesta sarjasta. Tekoälyagentti saa vain ehdottaa laskun kirjaamista; ihminen kirjaa sen.

Laskutus ei tee verkkolaskutusta. Se ei osaa tuottaa XRechnung-, Factur-X- tai Peppol BIS -tiedostoa eikä lukea sellaista, eikä se tarkista mitään EN 16931:tä vasten. Laskussa ei myöskään vielä ole arvonlisäveron erittelyä, ja standardi vaatii sellaisen.

## Mitä rakennamme

Työssä on kaksi puoliskoa. Aloitamme lukemisesta, koska yritysten on vuodesta 2025 alkaen pitänyt olla valmiita ottamaan vastaan verkkolaskuja:

1. **Lukeminen ja tarkistus.** Saapuvat UBL- ja CII-laskut, myös Peppol BIS Billing 3.0, luetaan yhteen EN 16931 -tietomalliin, joka kattaa otsikon (numero, päivämäärä, tyyppi, valuutta), myyjän ja ostajan, rivit, arvonlisäveron erittelyn ja summat, ja ne tarkistetaan standardin sääntöjä, Saksan XRechnung-sääntöjä ja Peppol BIS -sääntöjä vasten. Testit käyttävät virallisia esimerkkilaskuja, jokaisella odottamamme tulos.
2. **Kirjoittaminen ja lähettäminen.** obop-laskusta tehdään XRechnung- tai Factur-X-tiedosto, ruotsalaisille asiakkaille Peppol BIS Billing 3.0 -tiedosto, ja se lähetetään sähköpostilla tai tiedostona.

Lähettäminen Peppol-verkon kautta tulee myöhemmin, suunnitelman toisessa aallossa. Se kulkee sertifioidun liityntäpisteen kautta, jonka yritys valitsee itse, yrityksen omilla tunnuksilla. SpaceCorps ei aio ylläpitää liityntäpistettä.

Kuten kaikessa obopissa, jokaisesta vaiheesta tulee komento, joka toimii samalla tavalla komentorivillä (CLI), REST API:ssa ja MCP:ssä, joten tekoälyagentti voi lukea saapuvan laskun tai valmistella lähtevän yhtä helposti kuin ihminen.

## Päivämäärät

Saksan paketin vakaan version tavoitepäivä on 2027-09-30, kolme kuukautta ennen kuin velvoite koskee kaikkia. Alfa- (2027-03-31) ja beta-päivämäärät (2027-06-30) ovat ehdotuksia. Pakettia kutsutaan vakaaksi vasta, kun sen testitapaukset menevät läpi ja kirjanpitäjä on hyväksynyt sen. Jos näin ei ole käynyt, se pysyy betana ja päivämäärä pysyy paikallaan.

## Mitä seuraavaksi

Ensimmäinen vaihe on lukija: UBL-laskut, myös Peppol BIS, luetaan EN 16931 -malliin ja testataan virallisia esimerkkejä vasten. Sen jälkeen tulee CII ja sitten tarkistukset. Seuraava kirjoitus näyttää lukijan toiminnassa virallisilla esimerkkilaskuilla.

Voit seurata työtä [repositoriossa](https://github.com/SpaceCorps/obop), jossa [Saksan paketin suunnitelma](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) ja sen taustalla olevat [päätökset](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) ovat julkisia.

## Lähteet

(haettu 2026-11-25)

- [§ 14 UStG, Ausstellung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 14b UStG, Aufbewahrung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14b.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 27 UStG, Allgemeine Übergangsvorschriften (Absatz 38)](https://www.gesetze-im-internet.de/ustg_1980/__27.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 33 UStDV, Rechnungen über Kleinbeträge](https://www.gesetze-im-internet.de/ustdv_1980/__33.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/), Sveriges riksdag
- [Regler för e-handel och e-faktura](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), Upphandlingsmyndigheten
- [E-faktura från statliga myndigheter](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/), Upphandlingsmyndigheten
- [XRechnung](https://xeinkauf.de/xrechnung/), XStandards Einkauf (KoSIT)
- [ZUGFeRD/Factur-X](https://www.ferd-net.de/standards/zugferd), FeRD
- [ZUGFeRD 2.5.2 english](https://www.ferd-net.de/publikationen-produkte/publikationen/detailseite/zugferd-252-english), FeRD
- [Peppol BIS Billing 3.0](https://docs.peppol.eu/poacc/billing/3.0/), OpenPeppol
- [docs/LOCALIZATION.md](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md), obop
- [docs/DECISIONS.md](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md), obop
