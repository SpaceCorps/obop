---
title: "E-Rechnung für Deutschland und Schweden: woran wir bauen"
description: "Deutschland macht die strukturierte elektronische Rechnung ab 2028 für jedes Unternehmen zur Pflicht, und öffentliche Auftraggeber in Schweden empfangen sie schon heute. So will obop sie lesen, prüfen und schreiben, und das gibt es bereits."
date: "2026-11-25"
author: "Abhinav Singh"
---

Ab 2028-01-01 muss jedes in Deutschland ansässige Unternehmen seine Rechnungen an andere Unternehmen in Deutschland als strukturierte elektronische Rechnungen senden. Ein PDF genügt dann nicht mehr. Das ist der Hauptgrund, warum Deutschland das erste Land von obop ist, und die elektronische Rechnung ist die Arbeit, mit der wir jetzt begonnen haben. Schweden verlangt für Rechnungen an öffentliche Auftraggeber dieselbe europäische Norm. Dieser Beitrag sagt, warum die Arbeit nötig ist, was das Gesetz in Deutschland und Schweden verlangt, was obop heute hat und was davon zu erwarten ist, wie eine elektronische Rechnung aufgebaut ist, wie die beiden wichtigsten Bildschirme aussehen werden, welche Termine gelten und wie es weitergeht.

## Warum das nötig ist

Eine Rechnungs-App, die keine elektronische Rechnung schreiben kann, ist in Deutschland für Rechnungen an andere Unternehmen nicht mehr nutzbar. Ab 2027-01-01 gilt das für Unternehmen mit mehr als 800.000 EUR Umsatz im vorangegangenen Jahr, ab 2028-01-01 für jedes Unternehmen. Empfangen können muss jedes Unternehmen elektronische Rechnungen schon seit 2025-01-01. Ohne elektronische Rechnung könnte ein deutsches Unternehmen obop nach 2027 oder 2028, je nach seiner Größe, nicht mehr für Rechnungen an andere Unternehmen nutzen.

In Schweden gilt dieselbe europäische Norm für Rechnungen an öffentliche Auftraggeber. Ein schwedisches Unternehmen, das an eine Gemeinde, eine Region oder eine staatliche Behörde verkauft, braucht sie heute.

Viele Unternehmen sind noch nicht bereit. In einem Bericht von 2026 der Firma Quadient, eines Anbieters von Software für die elektronische Rechnung, der auf mehr als 300 deutschen B2B-Unternehmen beruht, waren 89 Prozent zuversichtlich, die Fristen einzuhalten, aber nur 6 Prozent erfüllten die betrieblichen Anforderungen vollständig.

## Was das Gesetz verlangt

Das deutsche Recht (§ 14 UStG) definiert eine elektronische Rechnung als eine Rechnung, die in einem strukturierten elektronischen Format ausgestellt, übermittelt und empfangen wird. Dieses Format muss der europäischen Norm für die elektronische Rechnungsstellung, EN 16931, entsprechen oder ein anderes Format sein, das beide Seiten vereinbaren und das eine elektronische Verarbeitung weiterhin ermöglicht. Rechnungen sind acht Jahre aufzubewahren, wobei die Echtheit der Herkunft, die Unversehrtheit des Inhalts und die Lesbarkeit gewährleistet sein müssen (§ 14b UStG). obop wird die XML-Datei einer empfangenen elektronischen Rechnung unverändert aufbewahren. Die Pflicht kommt in Stufen:

- **Seit 2025-01-01:** Jedes Unternehmen muss elektronische Rechnungen empfangen können.
- **Ab 2027-01-01:** Unternehmen mit mehr als 800.000 EUR Umsatz im vorangegangenen Jahr müssen sie ausstellen.
- **Ab 2028-01-01:** Jedes Unternehmen muss sie ausstellen.

Ausnahmen gibt es unter anderem für Rechnungen bis 250 EUR (§ 33 UStDV) und, bis Ende 2027, für mit dem Empfänger vereinbarte EDI-Verfahren (§ 27 Abs. 38 UStG).

Deutschland akzeptiert mehrere Formate. obop will drei davon unterstützen: XRechnung, ZUGFeRD 2.5.2 (dasselbe wie Factur-X 1.09.2, ein PDF mit dem XML darin) und Peppol BIS Billing 3.0. Hinter diesen Formaten stehen zwei XML-Syntaxen, UBL und CII, und beide tragen denselben Inhalt nach EN 16931.

## Schweden

In Schweden gilt die Pflicht für Rechnungen an den öffentlichen Sektor. Das [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) trat am 2019-04-01 in Kraft. Für Vergabeverfahren, die ab diesem Tag begonnen wurden, müssen die Rechnungen eines Lieferanten an einen öffentlichen Auftraggeber elektronische Rechnungen sein, die der europäischen Norm EN 16931 entsprechen (4 §), und öffentliche Auftraggeber müssen solche Rechnungen empfangen und bearbeiten (5 §). Lieferant und Auftraggeber können eine andere Norm vereinbaren. Das Gesetz nennt kein Format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), seit 2026-07-01 für die zugehörigen Vorschriften zuständig, empfiehlt Peppol BIS Billing 3, eine Umsetzung von EN 16931, und [staatliche Behörden müssen es unterstützen](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Öffentliche Auftraggeber müssen in Peppol als Rechnungsempfänger registriert sein und sowohl UBL als auch CII empfangen können.

## Was obop heute hat

Heute verwaltet die App Rechnungsstellung von obop Kundenrechnungen und ihre Positionen, rechnet die Summe zusammen und gibt jeder gebuchten Rechnung die nächste Nummer einer lückenlosen Jahresreihe. Gebucht wird von einem Menschen; ein KI-Agent darf es vorschlagen. Als Nächstes kommt die elektronische Rechnung dazu: Dateien nach EN 16931 lesen, prüfen und schreiben, mit einer Aufschlüsselung der Umsatzsteuer nach Steuersatz.

## Was von obop zu erwarten ist

Das kommt, Schritt für Schritt.

**Empfangen.** Ein Mensch oder ein KI-Agent, der für ihn arbeitet, wird obop eine Datei mit einer elektronischen Rechnung geben: eine XML-Datei oder ein ZUGFeRD-PDF mit dem XML darin. obop wird sie lesen und gegen die Regeln von EN 16931 und ihres Formats prüfen: dass die Summen aufgehen und dass die Pflichtfelder vorhanden sind. Es wird auch prüfen, dass die Eingangsrechnung noch nicht gebucht wurde, anhand von Verkäufer, Nummer, Betrag und Datum. Das Ergebnis wird der Entwurf einer Eingangsrechnung sein. Die Datei selbst wird unverändert aufbewahrt, mit ihrem Fingerabdruck (einem SHA-256-Hash).

**Senden.** Wenn ein Mensch eine Rechnung bucht, wird obop zuerst prüfen, dass ihre Summen stimmen, und ihr erst dann ihre Nummer geben. Aus der gebuchten Rechnung wird es für deutsche Kunden eine XRechnung- oder ZUGFeRD-Datei machen, diese Datei gegen die Regeln prüfen, sie unverändert aufbewahren und sie per E-Mail senden oder als Datei speichern. Das Lesen von Rechnungen nach Peppol BIS Billing 3.0 gehört zur laufenden Arbeit; das Schreiben solcher Rechnungen für schwedische Kunden ist für später geplant. Eine abgelehnte Rechnung wird ihre Nummer behalten; die Korrektur wird eine Gutschrift und eine neue Rechnung sein. Der Versand über das Peppol-Netz kommt später, in der zweiten Welle des Plans. Er wird über einen zertifizierten Zugangspunkt (Access Point) laufen, den das Unternehmen selbst wählt, mit seinen eigenen Zugangsdaten. SpaceCorps wird keinen Zugangspunkt betreiben.

**KI-Agenten.** Ein KI-Agent wird eine elektronische Rechnung und das Ergebnis ihrer Prüfungen lesen und daraus den Entwurf einer Eingangsrechnung vorbereiten können. Das Buchen einer Rechnung oder einer Eingangsrechnung bleibt bei Menschen: Ein Agent wird es vorschlagen, und ein Mensch wird buchen. Wenn ein Agent darum bittet, eine elektronische Rechnung zu senden, wird der Versand warten, bis ein Mensch ihn bestätigt. Wie alles in obop wird jeder Schritt ein Befehl sein, der in der CLI, der REST-API und über MCP gleich funktioniert. So kann ein KI-Agent eine eingehende Rechnung ebenso leicht lesen oder eine ausgehende vorbereiten wie ein Mensch.

## Wie eine elektronische Rechnung aufgebaut ist

- **EN 16931** ist der Inhalt: welche Angaben eine Rechnung trägt und was sie bedeuten. Jede hat eine Nummer, etwa BT-1 für die Rechnungsnummer oder BT-2 für das Rechnungsdatum.
- **UBL und CII** sind zwei XML-Formate, die diesen Inhalt tragen können. Beide sind erlaubt, und ein Empfänger muss beide verarbeiten können.
- **XRechnung und Peppol BIS Billing 3.0** setzen die Regeln eines Landes oder eines Netzes obendrauf: welche Felder Pflicht sind, welche Codes erlaubt sind. XRechnung ist die deutsche; Peppol BIS wird in ganz Europa verwendet und in Schweden empfohlen.
- **E-Mail, Datei und Peppol** sind Wege, die Datei zu übermitteln. E-Mail und Datei brauchen keinen Vertrag mit irgendwem. Peppol ist ein Netz, dem man über einen Zugangspunkt beitritt.

## Designvorschauen

Wir haben die beiden Bildschirme entworfen, die Menschen benutzen werden.

<figure class="mockup">
<div class="mockup-label">Designvorschau</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">Eingehende elektronische Rechnung · rechnung-4711.pdf</span><span class="mockup-tag">ZUGFeRD · EN 16931</span></div>
<dl class="mockup-rows">
<div><dt>Verkäufer</dt><dd>Müller Bürobedarf GmbH (TEST) · DE000000001</dd></div>
<div><dt>Rechnungsnummer</dt><dd>RE-2026-4711</dd></div>
<div><dt>Ausgestellt · Fällig</dt><dd>2026-11-12 · 2026-12-12</dd></div>
<div><dt>Netto · USt · Zahlbetrag</dt><dd>1.000,00 € · 190,00 € · 1.190,00 €</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Bestanden: </span>Summen gehen auf</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Bestanden: </span>Noch nicht gebucht</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Warnung: </span>Verkäufer ist noch kein Kontakt</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Entwurf einer Eingangsrechnung anlegen</span><span class="mockup-btn">Verwerfen</span></div>
</div>
<figcaption>Empfangen: eine empfangene elektronische Rechnung, nachdem obop sie gelesen und geprüft hat, bevor ein Mensch den Entwurf der Eingangsrechnung anlegt. Beispieldaten.</figcaption>
</figure>

<figure class="mockup">
<div class="mockup-label">Designvorschau</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">INV-2026-00042 · Schmidt Logistik AG (TEST)</span><span class="mockup-tag">Gebucht</span></div>
<dl class="mockup-rows">
<div><dt>Gesamt inkl. USt</dt><dd>2.380,00 €</dd></div>
<div><dt>Format der elektronischen Rechnung</dt><dd>XRechnung (CII) · ZUGFeRD PDF</dd></div>
<div><dt>Käuferreferenz (BT-10)</dt><dd>PO-88213</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Bestanden: </span>Regeln von EN 16931 erfüllt</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Bestanden: </span>Jede Position hat eine Steuer und eine USt-Kategorie</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Warnung: </span>USt-IdNr. des Käufers fehlt</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">XRechnung herunterladen</span><span class="mockup-btn">ZUGFeRD-PDF herunterladen</span></div>
</div>
<figcaption>Senden: eine gebuchte Rechnung mit ihren Dateien als elektronische Rechnung, gegen die Regeln geprüft, bevor jemand sie herunterlädt. Beispieldaten.</figcaption>
</figure>

## Die Termine

Das deutsche Paket hat für seine stabile Version das Zieldatum 2027-09-30, drei Monate bevor die Pflicht für alle gilt. Die Termine für Alpha (2027-03-31) und Beta (2027-06-30) sind Vorschläge. Stabil heißt das Paket erst, wenn seine Testfälle bestehen und eine Buchhalterin oder ein Buchhalter es abgenommen hat. Ist das nicht geschehen, bleibt es Beta, und das Datum bleibt, wo es ist.

## Wie es weitergeht

Das Lesemodul ist in aktiver Entwicklung: Es liest bereits den Kopf jeder UBL-Rechnung in unserem Testsatz aus 119 Beispieldateien und weist zu große Dateien und unsicheres XML zurück. Als Nächstes liest es Verkäufer und Käufer, die Positionen, die Aufschlüsselung der Umsatzsteuer und die Summen. Danach folgt CII, dann die Prüfungen.

Sie können die Arbeit im [Repository](https://github.com/SpaceCorps/obop) verfolgen; dort sind der [Plan für das deutsche Paket](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) und die [Entscheidungen](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) dahinter öffentlich.

## Quellen

(abgerufen am 2026-11-25, und am 2026-11-28 für die markierten)

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
