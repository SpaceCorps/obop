---
title: "E-Rechnung für Deutschland und Schweden: woran wir bauen"
description: "Deutschland macht die strukturierte elektronische Rechnung ab 2028 für jedes Unternehmen zur Pflicht, und öffentliche Auftraggeber in Schweden empfangen sie schon heute. So will obop sie lesen, prüfen und schreiben, und das gibt es bereits."
date: "2026-11-25"
---

Ab 2028-01-01 muss jedes in Deutschland ansässige Unternehmen seine Rechnungen an andere Unternehmen in Deutschland als strukturierte elektronische Rechnungen senden. Ein PDF genügt dann nicht mehr. Das ist der Hauptgrund, warum Deutschland das erste Land von obop ist, und die elektronische Rechnung ist die Arbeit, mit der wir jetzt begonnen haben. Schweden verlangt für Rechnungen an öffentliche Auftraggeber dieselbe europäische Norm. Dieser Beitrag sagt, was das Gesetz verlangt, was obop heute hat und woran wir bauen.

## Was das Gesetz verlangt

Das deutsche Recht (§ 14 UStG) definiert eine elektronische Rechnung als eine Rechnung, die in einem strukturierten elektronischen Format ausgestellt, übermittelt und empfangen wird. Dieses Format muss der europäischen Norm für die elektronische Rechnungsstellung, EN 16931, entsprechen oder ein anderes Format sein, das beide Seiten vereinbaren und das eine elektronische Verarbeitung weiterhin ermöglicht. Rechnungen sind acht Jahre aufzubewahren, wobei die Echtheit der Herkunft, die Unversehrtheit des Inhalts und die Lesbarkeit gewährleistet sein müssen (§ 14b UStG). obop bewahrt die XML-Datei einer empfangenen elektronischen Rechnung unverändert auf. Die Pflicht kommt in Stufen:

- **Seit 2025-01-01:** Jedes Unternehmen muss elektronische Rechnungen empfangen können.
- **Ab 2027-01-01:** Unternehmen mit mehr als 800.000 EUR Umsatz im vorangegangenen Kalenderjahr müssen sie ausstellen.
- **Ab 2028-01-01:** Jedes Unternehmen muss sie ausstellen.

Ausnahmen gibt es unter anderem für Rechnungen bis 250 EUR (§ 33 UStDV) und, bis Ende 2027, für mit dem Empfänger vereinbarte EDI-Verfahren (§ 27 Abs. 38 UStG).

Deutschland akzeptiert mehrere Formate. obop will drei davon unterstützen: XRechnung, ZUGFeRD 2.5.2 (dasselbe wie Factur-X 1.09.2, ein PDF mit dem XML darin) und Peppol BIS Billing 3.0. Hinter diesen Formaten stehen zwei XML-Syntaxen, UBL und CII, und beide tragen denselben Inhalt nach EN 16931.

## Schweden

In Schweden gilt die Pflicht für Rechnungen an den öffentlichen Sektor. Das [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) trat am 2019-04-01 in Kraft. Für Vergabeverfahren, die ab diesem Tag begonnen wurden, müssen die Rechnungen eines Lieferanten an einen öffentlichen Auftraggeber elektronische Rechnungen sein, die der europäischen Norm EN 16931 entsprechen (4 §), und öffentliche Auftraggeber müssen solche Rechnungen empfangen und bearbeiten (5 §). Lieferant und Auftraggeber können eine andere Norm vereinbaren. Das Gesetz nennt kein Format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), seit 2026-07-01 für die zugehörigen Vorschriften zuständig, empfiehlt Peppol BIS Billing 3, eine Umsetzung von EN 16931, und [staatliche Behörden müssen es unterstützen](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Öffentliche Auftraggeber müssen in Peppol als Rechnungsempfänger registriert sein und sowohl UBL als auch CII empfangen können.

## Was obop heute hat

obop hat die App Rechnungsstellung. Sie verwaltet Kundenrechnungen und ihre Positionen, rechnet die Summe zusammen und gibt jeder gebuchten Rechnung die nächste Nummer einer lückenlosen Jahresreihe. Ein KI-Agent darf das Buchen einer Rechnung nur vorschlagen; gebucht wird sie von einem Menschen.

Rechnungsstellung kann noch keine elektronischen Rechnungen. Sie kann keine XRechnung-, Factur-X- oder Peppol-BIS-Datei erzeugen und keine lesen, und sie prüft nichts gegen EN 16931. Die Rechnung hat außerdem noch keine Aufschlüsselung der Umsatzsteuer, und die Norm verlangt eine.

## Woran wir bauen

Die Arbeit hat zwei Hälften. Wir beginnen mit dem Lesen, weil Unternehmen seit 2025 elektronische Rechnungen empfangen können müssen:

1. **Lesen und prüfen.** Eingehende UBL- und CII-Rechnungen, Peppol BIS Billing 3.0 eingeschlossen, in ein Datenmodell nach EN 16931 einlesen, mit dem Kopf (Nummer, Datum, Art, Währung), Verkäufer und Käufer, den Positionen, der Aufschlüsselung der Umsatzsteuer und den Summen, und sie gegen die Regeln der Norm, die deutschen XRechnung-Regeln und die Peppol-BIS-Regeln prüfen. Die Tests verwenden die offiziellen Beispielrechnungen, jede mit dem Ergebnis, das wir erwarten.
2. **Schreiben und senden.** Aus einer obop-Rechnung eine XRechnung- oder Factur-X-Datei machen, für schwedische Kunden eine Datei nach Peppol BIS Billing 3.0, und sie per E-Mail oder als Datei senden.

Der Versand über das Peppol-Netz kommt später, in der zweiten Welle des Plans. Er läuft über einen zertifizierten Zugangspunkt (Access Point), den das Unternehmen selbst wählt, mit seinen eigenen Zugangsdaten. SpaceCorps wird keinen Zugangspunkt betreiben.

Wie alles in obop wird jeder Schritt ein Befehl sein, der in der CLI, der REST-API und über MCP gleich funktioniert. So kann ein KI-Agent eine eingehende Rechnung ebenso leicht lesen oder eine ausgehende vorbereiten wie ein Mensch.

## Die Termine

Das deutsche Paket hat für seine stabile Version das Zieldatum 2027-09-30, drei Monate bevor die Pflicht für alle gilt. Die Termine für Alpha (2027-03-31) und Beta (2027-06-30) sind Vorschläge. Stabil heißt das Paket erst, wenn seine Testfälle bestehen und eine Buchhalterin oder ein Buchhalter es abgenommen hat. Ist das nicht geschehen, bleibt es Beta, und das Datum bleibt, wo es ist.

## Wie es weitergeht

Der erste Schritt ist der Leser: UBL-Rechnungen, Peppol BIS eingeschlossen, werden in das Modell nach EN 16931 eingelesen und gegen die offiziellen Beispiele getestet. Danach folgt CII, dann die Prüfungen. Der nächste Beitrag zeigt den Leser an den offiziellen Beispielrechnungen.

Sie können die Arbeit im [Repository](https://github.com/SpaceCorps/obop) verfolgen; dort sind der [Plan für das deutsche Paket](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) und die [Entscheidungen](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) dahinter öffentlich.

## Quellen

(abgerufen am 2026-11-25)

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
