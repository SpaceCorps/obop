---
title: "Plattform"
description: "So funktioniert die obop-Plattform: Instanzen mit einer Datenbank pro Unternehmen, Module und der Marktplatz, Berechtigungsgruppen, Datenklassen und Maskierung, Freigaben sowie die Schnittstellen MCP, REST und CLI."
lead: "Ein Programm, ein Datenmodell, drei Zugänge. Diese Seite erklärt, wie die Teile zusammenpassen."
eyebrow: "So funktioniert es"
---

## Eine Instanz pro Unternehmen

Jedes Unternehmen läuft in einer eigenen **Instanz** mit eigener Datenbank, eigenen Benutzern und eigener Adresse. Beim gehosteten Dienst legt eine Control Plane die Instanz nach der Registrierung für Sie an und teilt Ihnen die Adresse mit; die Person, die sich registriert hat, wird ihr erster Administrator. Sie können dasselbe Programm auch selbst betreiben. Siehe [Instanzen](/docs/concepts/instances/).

Als Speicher dient SQLite zum Ausprobieren und PostgreSQL für den echten Einsatz. Datensätze werden vorerst als JSON-Dokumente gespeichert; die Befehlsschicht prüft die Feldtypen, ein Schema-Manager ist geplant.

## Module sind Apps

Eine App ist ein **Modul**: eine Beschreibung von Modellen, Feldern, Berechtigungsgruppen und Workflow-Aktionen. Sechs integrierte Apps haben eine eigene Seite auf dieser Website (Contacts, Products, CRM, Sales, Invoicing, Connectors). 24 weitere werden als Pakete mitgeliefert, 22 davon ebenfalls mit einer eigenen Seite. Sie fügen Apps mit einem Klick aus dem Marktplatz hinzu oder laden eine eigene Datei hoch.

Weil jedes Modul durch eine gemeinsame Registry läuft, ist ein Kontakt in CRM, Sales und Invoicing derselbe Kontakt, und jedes Modul erhält dieselbe API, dieselben Oberflächen, Berechtigungen und Datenschutzregeln. Siehe [Apps und Module](/docs/concepts/apps-and-modules/) und die [Liste der Apps](/apps/).

## Marktplatz

Der Marktplatz ist ein statischer Host, der `index.json` und je Paket eine `obop.toml` ausliefert. Der mitgelieferte Katalog ist Teil des Programms; Sie können eine Instanz auch auf Ihren eigenen Host verweisen lassen. Jeder Download wird wie ein Upload geprüft. Siehe [Marktplatz](/marketplace/).

## Berechtigungen

Berechtigungsgruppen wie `crm.user` oder `invoicing.manager` werden von Modellen, Feldern und Aktionen benannt. Personen haben Gruppen; Agent-Tokens haben eine Teilmenge der Gruppen ihres Besitzers. Siehe [Berechtigungsgruppen](/docs/concepts/permission-groups/).

## Datenklassen und Maskierung

Jedes Feld legt eine Datenklasse fest (none, public directory, internal, confidential HR, special category, financial, secret) und ob es sich um personenbezogene Daten handelt. Menschen sehen Werte im Rahmen ihrer Berechtigungen. Agenten sehen personenbezogene Daten als stabile Platzhalter und sensible Klassen gar nicht, und sie können Platzhalter zurückgeben, um sich auf einen Wert zu beziehen, ohne ihn zu sehen. Siehe [Datenklassen und Maskierung](/docs/concepts/data-classes-and-masking/).

## Workflows und Freigaben

Zustandsänderungen sind benannte Aktionen. Eine Aktion kann eine strengere Gruppe und eine Freigabe verlangen. Eine Aktion, die eine Freigabe braucht, kann ein Agent nur vorschlagen: Der Server liefert einen Plan zurück und ändert nichts, bis eine Person ihn freigibt. Auch das Löschen von Datensätzen schlagen Agenten nur vor. Siehe [Workflows und Freigaben](/docs/concepts/workflows-and-approvals/).

## Drei Schnittstellen, ein Satz von Befehlen

| Schnittstelle | Für                       | Wie                                                                                                       |
| ------------- | ------------------------- | --------------------------------------------------------------------------------------------------------- |
| Web-App       | Menschen                  | Aus den Modulbeschreibungen erzeugt: Listen, Boards, Formulare.                                           |
| REST          | Skripte, Dienste, Agenten | `POST /api/{model}/{command}` mit einem Bearer-Token. [REST-Referenz](/docs/reference/rest-api/)          |
| MCP           | KI-Agenten                | `obop mcp` über stdio oder `/mcp` über HTTP, ein Tool pro Befehl. [Über MCP verbinden](/docs/agents/mcp/) |
| CLI           | Shells und Cron           | `obop call model command key=value`. [CLI-Referenz](/docs/reference/cli/)                                 |

Alle rufen denselben Dispatcher auf, daher sind Berechtigungsprüfungen, Maskierung und Freigaben identisch.

## Was es (noch) nicht ist

Bedenken Sie das, bevor Sie sich festlegen:

- Es gibt keine Code-Stufen über deklarative Pakete hinaus (keine WebAssembly-Komponenten, keine Remote-Apps). Ein App-Paket führt also keinen eigenen Code aus und kann keine Aufrufe anderer Systeme deklarieren, kann aber berechnete Summen und Aktionen deklarieren, die andere Datensätze in derselben Datenbank ändern.
- Nur manche Apps (zum Beispiel Expenses, Timesheets, HR-Urlaubsanträge, Documents und Dashboards) beschränken, welche Datensätze eine Person sieht. CRM, Sales und Invoicing tun das nicht.
- Das Sicherheitslog erfasst zum Beispiel Ereignisse zu Konten, Anmeldungen, Sicherheitseinstellungen, Modulen und Signaturen, ist aber kein Verlauf der Änderungen an den Datensätzen des Unternehmens.
- Der geplante Exportauftrag für die Daten eines ganzen Unternehmens ist nicht gebaut. Eine Liste lässt sich in der Web-App als CSV exportieren (bis zu 20.000 Datensätze), und `obop backup --plain` schreibt die Datensätze und hochgeladenen Dateien einer Datenbank in eine unverschlüsselte Datei (kurzlebige Werte wie Sitzungen fehlen; Geheimnisse bleiben versiegelt, aber wenn Sie `OBOP_SECRET_KEY` nicht gesetzt haben, steht der Schlüssel, den obop für sie erzeugt hat, in der Datei).
- Der gehostete Dienst schaltet zeitgesteuerte Sicherungen seiner Datenbanken noch nicht ein (wer obop selbst betreibt, kann sie einschalten).

Die [Roadmap](/roadmap/) zeigt, was gebaut ist und was nicht.
