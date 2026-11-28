---
title: "E-faktura för Tyskland och Sverige: det här bygger vi"
description: "Tyskland gör strukturerade e-fakturor obligatoriska för alla företag från 2028, och svenska upphandlande myndigheter tar redan emot dem. Så här planerar obop att läsa, kontrollera och skriva dem, och det här finns i dag."
date: "2026-11-25"
author: "Abhinav Singh"
---

Från 2028-01-01 måste varje företag som är etablerat i Tyskland skicka sina fakturor till andra företag i Tyskland som strukturerade e-fakturor. En pdf räcker inte längre. Det är huvudskälet till att Tyskland är obops första land, och e-faktura är arbetet vi nu har börjat med. Sverige kräver samma europeiska standard för fakturor till upphandlande myndigheter. Det här inlägget berättar varför arbetet behövs, vad lagen kräver i Tyskland och Sverige, vad obop har i dag och vad du kan vänta dig av det, hur en e-faktura är uppbyggd, hur de två viktigaste skärmarna kommer att se ut, datumen och vad som kommer härnäst.

## Varför det behövs

En faktureringsapp som inte kan skriva en e-faktura går inte längre att använda i Tyskland för fakturor till andra företag. Från 2027-01-01 gäller det företag med en omsättning på mer än 800 000 EUR föregående år, och från 2028-01-01 alla företag. Alla företag måste redan kunna ta emot e-fakturor, sedan 2025-01-01. Utan e-faktura skulle ett tyskt företag inte kunna använda obop för att fakturera andra företag efter 2027 eller 2028, beroende på dess storlek.

I Sverige gäller samma europeiska standard för fakturor till upphandlande myndigheter. Ett svenskt företag som säljer till en kommun, en region eller en statlig myndighet behöver den redan i dag.

Många företag är inte redo än. I en rapport från 2026 av Quadient, en leverantör av programvara för e-faktura, som bygger på mer än 300 tyska B2B-företag, var 89 procent säkra på att de skulle klara tidsfristerna, men bara 6 procent hade helt uppfyllt de operativa kraven.

## Vad lagen kräver

Tysk lag (§ 14 UStG) definierar en e-faktura som en faktura som utfärdas, skickas och tas emot i ett strukturerat elektroniskt format. Formatet ska följa den europeiska standarden för e-fakturering, EN 16931, eller vara ett annat format som båda parter kommer överens om och som fortfarande kan behandlas elektroniskt. Fakturor ska sparas i åtta år, och deras ursprung, innehåll och läsbarhet ska vara säkrade (§ 14b UStG). obop kommer att spara XML-filen i en mottagen e-faktura oförändrad. Kravet kommer i steg:

- **Sedan 2025-01-01:** alla företag måste kunna ta emot e-fakturor.
- **Från 2027-01-01:** företag med en omsättning på mer än 800 000 EUR föregående år måste utfärda dem.
- **Från 2028-01-01:** alla företag måste utfärda dem.

Undantag finns bland annat för fakturor på högst 250 EUR (§ 33 UStDV) och, till och med slutet av 2027, för EDI-förfaranden som avtalats med mottagaren (§ 27 Abs. 38 UStG).

Tyskland godtar flera format. obop planerar att hantera tre av dem: XRechnung, ZUGFeRD 2.5.2 (samma som Factur-X 1.09.2, en pdf med XML-filen inuti) och Peppol BIS Billing 3.0. Bakom formaten finns två XML-syntaxer, UBL och CII, och båda bär samma innehåll enligt EN 16931.

## Sverige

I Sverige gäller kravet fakturor till den offentliga sektorn. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) trädde i kraft 2019-04-01. För upphandlingar som har påbörjats från och med den dagen ska en leverantörs fakturor till en upphandlande myndighet vara elektroniska fakturor som följer den europeiska standarden EN 16931 (4 §), och upphandlande myndigheter ska ta emot och behandla sådana fakturor (5 §). Leverantören och den upphandlande myndigheten får komma överens om en annan standard. Lagen anger inget format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), som sedan 2026-07-01 ansvarar för de tillhörande föreskrifterna, rekommenderar Peppol BIS Billing 3, en tillämpning av EN 16931, och [statliga myndigheter måste stödja den](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Upphandlande myndigheter ska vara registrerade i Peppol som fakturamottagare och kunna ta emot både UBL och CII.

## Det här har obop i dag

I dag håller obops app Fakturering kundfakturor och deras rader, räknar ihop totalen och ger varje bokförd faktura nästa nummer i en obruten årlig nummerserie. En människa bokför; en AI-agent får föreslå det. Det den får härnäst är e-faktura: att läsa, kontrollera och skriva filer enligt EN 16931, med en momsuppdelning per skattesats.

## Vad du kan vänta dig av obop

Här är det som kommer, steg för steg.

**Ta emot.** En människa, eller en AI-agent som arbetar för en människa, kommer att ge obop en e-fakturafil: en XML-fil, eller en ZUGFeRD-pdf med XML-filen inuti. obop kommer att läsa den och kontrollera den mot reglerna i EN 16931 och i dess format: att summorna går ihop och att de obligatoriska fälten finns. Den kommer också att kontrollera att leverantörsfakturan inte har bokförts tidigare, utifrån säljare, nummer, belopp och datum. Resultatet kommer att bli ett utkast till leverantörsfaktura. Själva filen kommer att sparas oförändrad, med sitt fingeravtryck (en SHA-256-hash).

**Skicka.** När en människa bokför en faktura kommer obop först att kontrollera att dess summor stämmer, och först därefter ge den dess nummer. Utifrån den bokförda fakturan kommer den att skapa en XRechnung- eller ZUGFeRD-fil för tyska kunder, kontrollera filen mot reglerna, spara den oförändrad och skicka den med e-post eller spara den som fil. Att läsa fakturor enligt Peppol BIS Billing 3.0 ingår i det pågående arbetet; att skriva dem för svenska kunder är planerat till senare. En avvisad faktura kommer att behålla sitt nummer; rättelsen kommer att vara en kreditfaktura och en ny faktura. Att skicka via Peppol-nätverket kommer senare, i planens andra våg. Det kommer att ske via en certifierad accesspunkt som företaget själv väljer, med sina egna inloggningsuppgifter. SpaceCorps kommer inte att driva någon accesspunkt.

**AI-agenter.** En AI-agent kommer att kunna läsa en e-faktura och resultatet av dess kontroller, och förbereda ett utkast till leverantörsfaktura utifrån den. Att bokföra en faktura eller en leverantörsfaktura kommer att ligga kvar hos människor: en agent kommer att föreslå det, och en människa kommer att bokföra. När en agent ber om att skicka en e-faktura kommer sändningen att vänta på att en människa bekräftar den. Som allt i obop kommer varje steg att vara ett kommando som fungerar likadant i CLI:t, REST-API:t och MCP, så att en AI-agent kan läsa en inkommande faktura eller förbereda en utgående lika lätt som en människa.

## Hur en e-faktura är uppbyggd

- **EN 16931** är innehållet: vilka uppgifter en faktura bär och vad de betyder. Varje uppgift har ett nummer, till exempel BT-1 för fakturanumret eller BT-2 för fakturadatumet.
- **UBL och CII** är två XML-format som kan bära det innehållet. Båda är tillåtna, och en mottagare måste hantera båda.
- **XRechnung och Peppol BIS Billing 3.0** lägger till ett lands eller ett nätverks regler ovanpå: vilka fält som är obligatoriska, vilka koder som är tillåtna. XRechnung är det tyska; Peppol BIS används i hela Europa och rekommenderas i Sverige.
- **E-post, fil och Peppol** är sätt att transportera filen. E-post och fil kräver inget avtal med någon. Peppol är ett nätverk som man ansluter sig till via en accesspunkt.

## Designförhandsvisningar

Vi har formgett de två skärmar som människor kommer att använda.

<figure class="mockup">
<div class="mockup-label">Designförhandsvisning</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">Inkommande e-faktura · rechnung-4711.pdf</span><span class="mockup-tag">ZUGFeRD · EN 16931</span></div>
<dl class="mockup-rows">
<div><dt>Säljare</dt><dd>Müller Bürobedarf GmbH (TEST) · DE000000001</dd></div>
<div><dt>Fakturanummer</dt><dd>RE-2026-4711</dd></div>
<div><dt>Utfärdad · Förfallodag</dt><dd>2026-11-12 · 2026-12-12</dd></div>
<div><dt>Netto · Moms · Att betala</dt><dd>1 000,00 € · 190,00 € · 1 190,00 €</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Godkänd: </span>Summorna går ihop</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Godkänd: </span>Inte bokförd tidigare</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Varning: </span>Säljaren är ännu ingen kontakt</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Skapa utkast till leverantörsfaktura</span><span class="mockup-btn">Kassera</span></div>
</div>
<figcaption>Ta emot: en mottagen e-faktura efter att obop har läst och kontrollerat den, innan en människa skapar utkastet till leverantörsfaktura. Exempeldata.</figcaption>
</figure>

<figure class="mockup">
<div class="mockup-label">Designförhandsvisning</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">INV-2026-00042 · Schmidt Logistik AG (TEST)</span><span class="mockup-tag">Bokförd</span></div>
<dl class="mockup-rows">
<div><dt>Totalt inkl. moms</dt><dd>2 380,00 €</dd></div>
<div><dt>E-fakturaformat</dt><dd>XRechnung (CII) · ZUGFeRD PDF</dd></div>
<div><dt>Köparens referens (BT-10)</dt><dd>PO-88213</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Godkänd: </span>Reglerna i EN 16931 uppfylls</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Godkänd: </span>Varje rad har en skatt och en momskategori</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Varning: </span>Köparens momsregistreringsnummer saknas</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Ladda ner XRechnung</span><span class="mockup-btn">Ladda ner ZUGFeRD-PDF</span></div>
</div>
<figcaption>Skicka: en bokförd faktura med sina e-fakturafiler, kontrollerade mot reglerna innan någon laddar ner dem. Exempeldata.</figcaption>
</figure>

## Datumen

Det tyska paketet har 2027-09-30 som måldatum för sin stabila version, tre månader innan kravet gäller alla. Datumen för alfa (2027-03-31) och beta (2027-06-30) är förslag. Paketet kallas stabilt först när dess testfall går igenom och en redovisningskonsult har godkänt det. Har det inte skett förblir det beta, och datumet ligger kvar.

## Vad som kommer härnäst

Läsmodulen är under aktiv utveckling: den läser redan fakturahuvudet i varje UBL-faktura i vår testsamling på 119 exempelfiler och avvisar för stora filer och osäker XML. Härnäst läser den säljare och köpare, raderna, momsuppdelningen och summorna. Sedan kommer CII och därefter kontrollerna.

Du kan följa arbetet i [repot](https://github.com/SpaceCorps/obop), där [planen för det tyska paketet](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) och [besluten](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) bakom den är offentliga.

## Källor

(hämtade 2026-11-25, och 2026-11-28 för de markerade)

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
