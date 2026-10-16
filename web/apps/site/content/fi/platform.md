---
title: "Alusta"
description: "Näin obop-alusta toimii: instanssit ja yksi tietokanta yritystä kohti, moduulit ja markkinapaikka, käyttöoikeusryhmät, tietoluokat ja maskaus, hyväksynnät sekä MCP-, REST- ja CLI-rajapinnat."
lead: "Yksi ohjelma, yksi tietomalli, kolme tapaa käyttää. Tällä sivulla kerrotaan, miten osat liittyvät toisiinsa."
eyebrow: "Näin se toimii"
---

## Yksi instanssi yritystä kohti

Jokainen yritys toimii omassa **instanssissaan**, jolla on oma tietokanta, omat käyttäjät ja oma osoite. Isännöidyssä palvelussa ohjaustaso luo instanssin rekisteröitymisen jälkeen ja antaa sinulle sen osoitteen; rekisteröityneestä henkilöstä tulee instanssin ensimmäinen ylläpitäjä. Voit myös ajaa samaa ohjelmaa itse. Katso [Instanssit](/docs/concepts/instances/).

Tallennuksena on SQLite kokeiluun ja PostgreSQL oikeaan käyttöön. Tietueet tallennetaan toistaiseksi JSON-dokumentteina; komentokerros tarkistaa kenttien tyypit, ja skeemanhallinta on suunnitteilla.

## Moduulit ovat sovelluksia

Sovellus on **moduuli**: kuvaus malleista, kentistä, käyttöoikeusryhmistä ja työnkulun toiminnoista. Viisi moduulia on sisäänrakennettuja (Contacts, Products, CRM, Sales, Invoicing). Kaksikymmentäkaksi muuta toimitetaan paketteina. Sovelluksia lisätään markkinapaikalta yhdellä napsautuksella, tai voit ladata oman tiedostosi.

Koska jokainen moduuli kulkee saman rekisterin kautta, yhteystieto on sama yhteystieto CRM:ssä, Salesissa ja Invoicingissa, ja jokainen moduuli saa samat rajapinnat, näkymät, käyttöoikeudet ja tietosuojasäännöt. Katso [Sovellukset ja moduulit](/docs/concepts/apps-and-modules/) ja [sovellusluettelo](/apps/).

## Markkinapaikka

Markkinapaikka on staattinen palvelin, joka tarjoaa tiedoston `index.json` ja yhden `obop.toml`-tiedoston pakettia kohti. Mukana tuleva luettelo toimitetaan ohjelman sisällä; instanssin voi osoittaa myös omalle palvelimellesi. Jokainen lataus validoidaan kuten oma latauksesi. Katso [Markkinapaikka](/marketplace/).

## Käyttöoikeudet

Käyttöoikeusryhmät, kuten `crm.user` tai `invoicing.manager`, nimetään mallien, kenttien ja toimintojen mukaan. Ihmisillä on ryhmiä; agenttitunnisteilla on osajoukko omistajansa ryhmistä. Katso [Käyttöoikeusryhmät](/docs/concepts/permission-groups/).

## Tietoluokat ja maskaus

Jokainen kenttä ilmoittaa tietoluokkansa (ei mikään, julkinen hakemisto, sisäinen, luottamuksellinen henkilöstötieto, erityisryhmä, talous, salaisuus) ja sen, onko se henkilötieto. Ihmiset näkevät arvot käyttöoikeuksiensa mukaan. Agentit näkevät henkilötiedot pysyvinä paikkamerkkeinä ja arkaluonteiset luokat eivät lainkaan, ja ne voivat välittää paikkamerkkejä takaisin viitatakseen arvoon näkemättä sitä. Katso [Tietoluokat ja maskaus](/docs/concepts/data-classes-and-masking/).

## Työnkulut ja hyväksynnät

Tilamuutokset ovat nimettyjä toimintoja. Toiminto voi vaatia tiukemman ryhmän ja hyväksynnän. Agentti voi vain ehdottaa toimintoa, joka vaatii hyväksynnän: palvelin palauttaa suunnitelman eikä muuta mitään, ennen kuin ihminen hyväksyy sen. Myös tietueiden poistamisen agentit vain ehdottavat. Katso [Työnkulut ja hyväksynnät](/docs/concepts/workflows-and-approvals/).

## Kolme rajapintaa, yksi komentojen joukko

| Rajapinta      | Kenelle                              | Miten                                                                                               |
| -------------- | ------------------------------------ | --------------------------------------------------------------------------------------------------- |
| Verkkosovellus | Ihmisille                            | Muodostetaan moduulien kuvauksista: luettelot, taulut, lomakkeet.                                   |
| REST           | Skripteille, palveluille, agenteille | `POST /api/{model}/{command}` Bearer-tunnisteella. [REST-viite](/docs/reference/rest-api/)          |
| MCP            | Tekoälyavustajille                   | `obop mcp` stdio-yhteydellä, yksi työkalu komentoa kohti. [Yhdistä MCP:n kautta](/docs/agents/mcp/) |
| CLI            | Komentoriville ja cronille           | `obop call model command key=value`. [CLI-viite](/docs/reference/cli/)                              |

Kaikki kutsuvat samaa jakelijaa, joten käyttöoikeustarkistukset, maskaus ja hyväksynnät ovat täsmälleen samat.

## Mitä se ei ole (vielä)

Tämä kannattaa tietää ennen sitoutumista: koodiportaita ei ole, joten moduulit eivät voi laskea kenttiä tai suorittaa sivuvaikutuksia; rivikohtaisia käyttöoikeuksia ei ole; auditointilokia, tietojen vientityökalua tai isännöidyn palvelun varmuuskopioita ei ole. [Tiekartta](/roadmap/) kertoo, mikä on valmista ja mikä ei.
