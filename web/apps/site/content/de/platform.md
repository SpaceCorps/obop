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

Eine App ist ein **Modul**: eine Beschreibung von Modellen, Feldern, Berechtigungsgruppen und Workflow-Aktionen. Fünf Module sind integriert (Contacts, Products, CRM, Sales, Invoicing). Zweiundzwanzig weitere werden als Pakete mitgeliefert. Sie fügen Apps mit einem Klick aus dem Marktplatz hinzu oder laden eine eigene Datei hoch.

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

| Schnittstelle | Für                       | Wie                                                                                              |
| ------------- | ------------------------- | ------------------------------------------------------------------------------------------------ |
| Web-App       | Menschen                  | Aus den Modulbeschreibungen erzeugt: Listen, Boards, Formulare.                                  |
| REST          | Skripte, Dienste, Agenten | `POST /api/{model}/{command}` mit einem Bearer-Token. [REST-Referenz](/docs/reference/rest-api/) |
| MCP           | KI-Assistenten            | `obop mcp` über stdio, ein Tool pro Befehl. [Über MCP verbinden](/docs/agents/mcp/)              |
| CLI           | Shells und Cron           | `obop call model command key=value`. [CLI-Referenz](/docs/reference/cli/)                        |

Alle rufen denselben Dispatcher auf, daher sind Berechtigungsprüfungen, Maskierung und Freigaben identisch.

## Was es (noch) nicht ist

Bedenken Sie das, bevor Sie sich festlegen: Es gibt keine Code-Stufen, Module können also keine Felder berechnen und keine Nebeneffekte ausführen; es gibt keine Berechtigungen auf Zeilenebene; es gibt kein Audit-Log, kein Werkzeug zum Datenexport und keine Backups für den gehosteten Dienst. Die [Roadmap](/roadmap/) zeigt, was gebaut ist und was nicht.
