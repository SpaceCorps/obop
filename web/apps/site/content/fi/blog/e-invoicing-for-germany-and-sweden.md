---
title: "Verkkolaskutus Saksaan ja Ruotsiin: mitä rakennamme"
description: "Saksa tekee rakenteisesta verkkolaskusta pakollisen kaikille yrityksille vuodesta 2028, ja Ruotsin julkiset hankkijat ottavat niitä jo vastaan. Näin obop aikoo lukea, tarkistaa ja kirjoittaa niitä, ja tämä on olemassa jo nyt."
date: "2026-11-25"
author: "Abhinav Singh"
---

Alkaen 2028-01-01 jokaisen Saksaan sijoittautuneen yrityksen on lähetettävä muille Saksassa toimiville yrityksille osoitetut laskunsa rakenteisina verkkolaskuina. PDF ei enää riitä. Tämä on tärkein syy siihen, että Saksa on obopin ensimmäinen maa, ja verkkolaskutus on työ, jonka olemme nyt aloittaneet. Ruotsi vaatii samaa eurooppalaista standardia julkisille hankkijoille lähetettäviltä laskuilta. Tämä kirjoitus kertoo, miksi työ on tarpeen, mitä laki vaatii Saksassa ja Ruotsissa, mitä obopissa on tänään ja mitä siltä voi odottaa, miten verkkolasku rakentuu, miltä kaksi tärkeintä näkymää tulevat näyttämään, mitkä päivämäärät ovat ja mitä seuraavaksi tapahtuu.

## Miksi tätä tarvitaan

Laskutussovellus, joka ei osaa kirjoittaa verkkolaskua, ei enää kelpaa Saksassa muille yrityksille lähetettäviin laskuihin. Alkaen 2027-01-01 tämä koskee yrityksiä, joiden liikevaihto edellisenä vuonna oli yli 800 000 EUR, ja alkaen 2028-01-01 jokaista yritystä. Jokaisen yrityksen on jo nyt pystyttävä vastaanottamaan verkkolaskuja, 2025-01-01 alkaen. Ilman verkkolaskutusta saksalainen yritys ei voisi käyttää obopia muiden yritysten laskuttamiseen vuodesta 2027 tai 2028 alkaen kokonsa mukaan.

Ruotsissa sama eurooppalainen standardi koskee julkisille hankkijoille lähetettäviä laskuja. Ruotsalainen yritys, joka myy kunnalle, alueelle tai valtion virastolle, tarvitsee sitä jo tänään.

Monet yritykset eivät ole vielä valmiita. Verkkolaskutusohjelmistoja myyvän Quadientin vuoden 2026 raportissa, joka perustuu yli 300 saksalaiseen B2B-yritykseen, 89 prosenttia luotti ehtivänsä määräaikoihin, mutta vain 6 prosenttia oli täyttänyt toiminnalliset vaatimukset kokonaan.

## Mitä laki vaatii

Saksan laki (§ 14 UStG) määrittelee verkkolaskun laskuksi, joka laaditaan, lähetetään ja vastaanotetaan rakenteisessa sähköisessä muodossa. Muodon on noudatettava eurooppalaista verkkolaskustandardia EN 16931 tai oltava jokin muu osapuolten sopima muoto, jota voi edelleen käsitellä sähköisesti. Laskut on säilytettävä kahdeksan vuotta niin, että niiden alkuperä, sisältö ja luettavuus on turvattu (§ 14b UStG). obop tulee säilyttämään vastaanotetun verkkolaskun XML-tiedoston muuttamattomana. Velvoite tulee voimaan vaiheittain:

- **Alkaen 2025-01-01:** jokaisen yrityksen on pystyttävä vastaanottamaan verkkolaskuja.
- **Alkaen 2027-01-01:** yritysten, joiden liikevaihto edellisenä vuonna oli yli 800 000 EUR, on laadittava niitä.
- **Alkaen 2028-01-01:** jokaisen yrityksen on laadittava niitä.

Poikkeuksia ovat muun muassa enintään 250 EUR:n laskut (§ 33 UStDV) sekä vuoden 2027 loppuun asti vastaanottajan kanssa sovitut EDI-menettelyt (§ 27 Abs. 38 UStG).

Saksa hyväksyy useita muotoja. obop aikoo tukea niistä kolmea: XRechnung, ZUGFeRD 2.5.2 (sama kuin Factur-X 1.09.2, PDF, jonka sisällä on XML) ja Peppol BIS Billing 3.0. Näiden muotojen taustalla on kaksi XML-syntaksia, UBL ja CII, ja molemmat kuljettavat samaa EN 16931 -sisältöä.

## Ruotsi

Ruotsissa velvoite koskee julkiselle sektorille lähetettäviä laskuja. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) tuli voimaan 2019-04-01. Siitä päivästä alkaen aloitetuissa hankinnoissa toimittajan julkiselle hankkijalle lähettämien laskujen on oltava eurooppalaista standardia EN 16931 noudattavia sähköisiä laskuja (4 §), ja julkisten hankkijoiden on otettava vastaan ja käsiteltävä tällaisia laskuja (5 §). Toimittaja ja hankkija voivat sopia toisesta standardista. Laki ei nimeä muotoa. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), joka on vastannut asiaan liittyvistä määräyksistä 2026-07-01 alkaen, suosittelee Peppol BIS Billing 3:a, joka on EN 16931:n toteutus, ja [valtion virastojen on tuettava sitä](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Julkisten hankkijoiden on oltava rekisteröityinä Peppoliin laskujen vastaanottajina, ja niiden on pystyttävä ottamaan vastaan sekä UBL että CII.

## Mitä obopissa on tänään

Tänään obopin Laskutus-sovellus pitää kirjaa asiakaslaskuista ja niiden riveistä, laskee loppusumman ja antaa jokaiselle kirjatulle laskulle seuraavan numeron aukottomasta vuosittaisesta sarjasta. Ihminen kirjaa laskun; tekoälyagentti voi ehdottaa sitä. Seuraavaksi se saa verkkolaskutuksen: EN 16931 -tiedostojen lukemisen, tarkistamisen ja kirjoittamisen sekä arvonlisäveron erittelyn verokannoittain.

## Mitä obopilta voi odottaa

Tässä on, mitä on tulossa, vaihe vaiheelta.

**Vastaanottaminen.** Jatkossa ihminen tai hänen puolestaan työskentelevä tekoälyagentti antaa obopille verkkolaskutiedoston: XML-tiedoston tai ZUGFeRD-PDF:n, jonka sisällä on XML. obop lukee sen ja tarkistaa sen EN 16931:n ja kyseisen muodon sääntöjä vasten: että summat täsmäävät ja että pakolliset kentät ovat mukana. Se tarkistaa myös, ettei ostolaskua ole kirjattu aiemmin, myyjän, numeron, summan ja päivämäärän perusteella. Tuloksena on ostolaskun luonnos. Itse tiedosto säilytetään muuttamattomana yhdessä sen sormenjäljen (SHA-256-tiivisteen) kanssa.

**Lähettäminen.** Kun ihminen kirjaa laskun, obop tarkistaa jatkossa ensin, että sen summat ovat oikein, ja antaa sille numeron vasta sitten. Kirjatusta laskusta se tekee XRechnung- tai ZUGFeRD-tiedoston saksalaisille asiakkaille, tarkistaa tiedoston sääntöjä vasten, säilyttää sen muuttamattomana ja lähettää sen sähköpostilla tai tallentaa sen tiedostoksi. Peppol BIS Billing 3.0 -laskujen lukeminen on osa nykyistä työtä; niiden kirjoittaminen ruotsalaisille asiakkaille on suunniteltu myöhemmäksi. Hylätty lasku säilyttää numeronsa; korjaus tehdään hyvityslaskulla ja uudella laskulla. Lähettäminen Peppol-verkon kautta tulee myöhemmin, suunnitelman toisessa aallossa. Se kulkee sertifioidun liityntäpisteen kautta, jonka yritys valitsee itse, yrityksen omilla tunnuksilla. SpaceCorps ei aio ylläpitää liityntäpistettä.

**Tekoälyagentit.** Tekoälyagentti voi jatkossa lukea verkkolaskun ja sen tarkistusten tuloksen sekä valmistella siitä ostolaskun luonnoksen. Laskun tai ostolaskun kirjaaminen jää ihmisille: agentti ehdottaa sitä, ja ihminen kirjaa. Kun agentti pyytää lähettämään verkkolaskun, lähetys odottaa, kunnes ihminen vahvistaa sen. Kuten kaikessa obopissa, jokaisesta vaiheesta tulee komento, joka toimii samalla tavalla komentorivillä (CLI), REST API:ssa ja MCP:ssä, joten tekoälyagentti voi lukea saapuvan laskun tai valmistella lähtevän yhtä helposti kuin ihminen.

## Miten verkkolasku rakentuu

- **EN 16931** on sisältö: mitä tietoja lasku sisältää ja mitä ne tarkoittavat. Jokaisella tiedolla on numero, kuten BT-1 laskun numerolle tai BT-2 laatimispäivälle.
- **UBL ja CII** ovat kaksi XML-muotoa, jotka voivat kuljettaa tätä sisältöä. Molemmat ovat sallittuja, ja vastaanottajan on osattava käsitellä molempia.
- **XRechnung ja Peppol BIS Billing 3.0** lisäävät päälle maan tai verkon säännöt: mitkä kentät ovat pakollisia ja mitkä koodit sallittuja. XRechnung on saksalainen; Peppol BIS on käytössä kaikkialla Euroopassa ja suositeltu Ruotsissa.
- **Sähköposti, tiedosto ja Peppol** ovat tapoja kuljettaa tiedosto. Sähköposti ja tiedosto eivät vaadi sopimusta kenenkään kanssa. Peppol on verkko, johon liitytään liityntäpisteen kautta.

## Suunnitelmien esikatselut

Olemme suunnitelleet kaksi näkymää, joita ihmiset tulevat käyttämään.

<figure class="mockup">
<div class="mockup-label">Suunnitelman esikatselu</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">Saapuva verkkolasku · rechnung-4711.pdf</span><span class="mockup-tag">ZUGFeRD · EN 16931</span></div>
<dl class="mockup-rows">
<div><dt>Myyjä</dt><dd>Müller Bürobedarf GmbH (TEST) · DE000000001</dd></div>
<div><dt>Laskun numero</dt><dd>RE-2026-4711</dd></div>
<div><dt>Päiväys · Eräpäivä</dt><dd>2026-11-12 · 2026-12-12</dd></div>
<div><dt>Veroton · ALV · Maksettava</dt><dd>1 000,00 € · 190,00 € · 1 190,00 €</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Hyväksytty: </span>Summat täsmäävät</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Hyväksytty: </span>Ei kirjattu aiemmin</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Varoitus: </span>Myyjä ei ole vielä yhteystieto</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Luo ostolaskun luonnos</span><span class="mockup-btn">Hylkää</span></div>
</div>
<figcaption>Vastaanotto: vastaanotettu verkkolasku sen jälkeen, kun obop on lukenut ja tarkistanut sen, ennen kuin ihminen luo ostolaskun luonnoksen. Esimerkkitiedot.</figcaption>
</figure>

<figure class="mockup">
<div class="mockup-label">Suunnitelman esikatselu</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">INV-2026-00042 · Schmidt Logistik AG (TEST)</span><span class="mockup-tag">Kirjattu</span></div>
<dl class="mockup-rows">
<div><dt>Yhteensä sis. ALV</dt><dd>2 380,00 €</dd></div>
<div><dt>Verkkolaskumuoto</dt><dd>XRechnung (CII) · ZUGFeRD PDF</dd></div>
<div><dt>Ostajan viite (BT-10)</dt><dd>PO-88213</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Hyväksytty: </span>EN 16931 -säännöt täyttyvät</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Hyväksytty: </span>Jokaisella rivillä on vero ja ALV-luokka</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Varoitus: </span>Ostajan ALV-numero puuttuu</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Lataa XRechnung</span><span class="mockup-btn">Lataa ZUGFeRD-PDF</span></div>
</div>
<figcaption>Lähetys: kirjattu lasku ja sen verkkolaskutiedostot, tarkistettu sääntöjä vasten ennen kuin kukaan lataa niitä. Esimerkkitiedot.</figcaption>
</figure>

## Päivämäärät

Saksan paketin vakaan version tavoitepäivä on 2027-09-30, kolme kuukautta ennen kuin velvoite koskee kaikkia. Alfa- (2027-03-31) ja beta-päivämäärät (2027-06-30) ovat ehdotuksia. Pakettia kutsutaan vakaaksi vasta, kun sen testitapaukset menevät läpi ja kirjanpitäjä on hyväksynyt sen. Jos näin ei ole käynyt, se pysyy betana ja päivämäärä pysyy paikallaan.

## Mitä seuraavaksi

Lukumoduuli on aktiivisessa kehityksessä: se lukee jo jokaisen UBL-laskun otsikon 119 esimerkkitiedoston testijoukossamme ja hylkää liian suuret tiedostot ja vaarallisen XML:n. Seuraavaksi se lukee myyjän ja ostajan, rivit, arvonlisäveron erittelyn ja summat. Sen jälkeen tulee CII ja sitten tarkistukset.

Voit seurata työtä [repositoriossa](https://github.com/SpaceCorps/obop), jossa [Saksan paketin suunnitelma](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) ja sen taustalla olevat [päätökset](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) ovat julkisia.

## Lähteet

(haettu 2026-11-25, merkityt lähteet 2026-11-28)

- [§ 14 UStG, Ausstellung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 14b UStG, Aufbewahrung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14b.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 27 UStG, Allgemeine Übergangsvorschriften (Absatz 38)](https://www.gesetze-im-internet.de/ustg_1980/__27.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 33 UStDV, Rechnungen über Kleinbeträge](https://www.gesetze-im-internet.de/ustdv_1980/__33.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [E-Rechnung Deutschland 2026: Die B2B-Halbzeitbilanz](https://www.quadient.com/de/ressourcen/e-rechnung-deutschland-bericht-2026), Quadient (2026-11-28)
- [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/), Sveriges riksdag
- [Regler för e-handel och e-faktura](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), Upphandlingsmyndigheten
- [E-faktura från statliga myndigheter](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/), Upphandlingsmyndigheten
- [XRechnung](https://xeinkauf.de/xrechnung/), XStandards Einkauf (KoSIT)
- [ZUGFeRD/Factur-X](https://www.ferd-net.de/standards/zugferd), FeRD
- [ZUGFeRD 2.5.2 english](https://www.ferd-net.de/publikationen-produkte/publikationen/detailseite/zugferd-252-english), FeRD
- [Peppol BIS Billing 3.0](https://docs.peppol.eu/poacc/billing/3.0/), OpenPeppol
- [docs/spec/finance.md (sections 4.6 and 5.7, the capability table and 'Agentic first')](https://github.com/SpaceCorps/obop/blob/main/docs/spec/finance.md), obop (2026-11-28)
- [docs/LOCALIZATION.md](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md), obop
- [docs/DECISIONS.md](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md), obop
