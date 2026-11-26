---
title: "E-faktura för Tyskland och Sverige: det här bygger vi"
description: "Tyskland gör strukturerade e-fakturor obligatoriska för alla företag från 2028, och svenska upphandlande myndigheter tar redan emot dem. Så här planerar obop att läsa, kontrollera och skriva dem, och det här finns i dag."
date: "2026-11-25"
---

Från 2028-01-01 måste varje företag som är etablerat i Tyskland skicka sina fakturor till andra företag i Tyskland som strukturerade e-fakturor. En pdf räcker inte längre. Det är huvudskälet till att Tyskland är obops första land, och e-faktura är arbetet vi nu har börjat med. Sverige kräver samma europeiska standard för fakturor inom offentlig upphandling. Det här inlägget berättar vad lagen kräver, vad obop har i dag och vad vi bygger.

## Vad lagen kräver

Tysk lag (§ 14 UStG) definierar en e-faktura som en faktura som utfärdas, skickas och tas emot i ett strukturerat elektroniskt format. Formatet ska följa den europeiska standarden för e-fakturering, EN 16931, eller vara ett annat format som båda parter kommer överens om och som fortfarande kan behandlas elektroniskt. Fakturor ska sparas i åtta år, och deras ursprung, innehåll och läsbarhet ska vara säkrade (§ 14b UStG). obop sparar XML-filen i en mottagen e-faktura oförändrad. Kravet kommer i steg:

- **Sedan 2025-01-01:** alla företag måste kunna ta emot e-fakturor.
- **Från 2027-01-01:** företag med en omsättning på mer än 800 000 EUR föregående år måste utfärda dem.
- **Från 2028-01-01:** alla företag måste utfärda dem.

Undantag finns bland annat för fakturor på högst 250 EUR (§ 33 UStDV) och, till och med slutet av 2027, för EDI-förfaranden som avtalats med mottagaren (§ 27 Abs. 38 UStG).

Tyskland godtar flera format. obop planerar att hantera tre av dem: XRechnung, ZUGFeRD 2.5.2 (samma som Factur-X 1.09.2, en pdf med XML-filen inuti) och Peppol BIS Billing 3.0. Bakom formaten finns två XML-syntaxer, UBL och CII, och båda bär samma innehåll enligt EN 16931.

## Sverige

I Sverige gäller kravet fakturor till den offentliga sektorn. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) trädde i kraft 2019-04-01. För upphandlingar som har påbörjats från och med den dagen ska en leverantörs fakturor till en upphandlande myndighet vara elektroniska fakturor som följer den europeiska standarden EN 16931 (4 §), och upphandlande myndigheter ska ta emot och behandla sådana fakturor (5 §). Leverantören och den upphandlande myndigheten får komma överens om en annan standard. Lagen anger inget format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), som sedan 2026-07-01 ansvarar för de tillhörande föreskrifterna, rekommenderar Peppol BIS Billing 3, en tillämpning av EN 16931, och [statliga myndigheter måste stödja den](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Upphandlande myndigheter ska vara registrerade i Peppol som fakturamottagare och kunna ta emot både UBL och CII.

## Det här har obop i dag

obop har appen Fakturering. Den håller kundfakturor och deras rader, räknar ihop totalen och ger varje bokförd faktura nästa nummer i en obruten årlig nummerserie. En AI-agent får bara föreslå att en faktura bokförs; det är en människa som bokför den.

Fakturering hanterar inte e-faktura. Den kan inte skapa en XRechnung-, Factur-X- eller Peppol BIS-fil, kan inte läsa en och kontrollerar ingenting mot EN 16931. Fakturan har heller ingen uppdelning av momsen än, och standarden kräver en.

## Det här bygger vi

Arbetet har två halvor. Vi börjar med att läsa, eftersom kravet att kunna ta emot har gällt sedan 2025:

1. **Läsa och kontrollera.** Läsa in inkommande UBL- och CII-fakturor, Peppol BIS Billing 3.0 inräknat, i en datamodell enligt EN 16931, med fakturahuvudet (nummer, datum, typ, valuta), säljare och köpare, rader, momsuppdelningen och summorna, och kontrollera dem mot standardens regler, de tyska XRechnung-reglerna och Peppol BIS-reglerna. Testerna använder de officiella exempelfakturorna, var och en med det resultat vi väntar oss.
2. **Skriva och skicka.** Göra om en obop-faktura till en XRechnung- eller Factur-X-fil, eller en Peppol BIS Billing 3.0-fil för svenska kunder, och skicka den med e-post eller som fil.

Att skicka via Peppol-nätverket kommer senare, i planens andra våg. Det sker via en certifierad accesspunkt som företaget själv väljer, med sina egna inloggningsuppgifter. SpaceCorps kommer inte att driva någon accesspunkt.

Som allt i obop blir varje steg ett kommando som fungerar likadant i CLI:t, REST-API:t och MCP, så att en AI-agent kan läsa en inkommande faktura eller förbereda en utgående lika lätt som en människa.

## Datumen

Det tyska paketet har 2027-09-30 som måldatum för sin stabila version, tre månader innan kravet gäller alla. Datumen för alfa (2027-03-31) och beta (2027-06-30) är förslag. Paketet kallas stabilt först när dess testfall går igenom och en redovisningskonsult har godkänt det. Har det inte skett förblir det beta, och datumet ligger kvar.

## Vad som kommer härnäst

Första steget är läsaren: UBL-fakturor, Peppol BIS inräknat, läses in i modellen enligt EN 16931 och testas mot de officiella exemplen. Sedan kommer CII och därefter kontrollerna. Nästa inlägg visar läsaren på de officiella exempelfakturorna.

Du kan följa arbetet i [repot](https://github.com/SpaceCorps/obop), där [planen för det tyska paketet](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) och [besluten](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) bakom den är offentliga.

## Källor

(hämtade 2026-11-25)

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
