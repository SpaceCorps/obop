---
title: "KI-Agenten"
description: "So richten Sie KI-Agenten für obop ein: das Agentenmodell, Tokens mit eingeschränktem Umfang, maskierte personenbezogene Daten und Freigaben, die einen Menschen bei allem Unumkehrbaren in der Verantwortung halten."
lead: "obop behandelt KI-Agenten als gleichberechtigte Nutzer, die bewusst eingeschränkt sind. Diese Seite erklärt das Modell; die nächsten Seiten zeigen, wie Sie einen Agenten einrichten."
---

## Das Agentenmodell

Ein **Agent** ist alles, was mit einem Agent-Token auf obop zugreift: ein MCP-Client wie Claude Desktop oder Cursor, ein Skript, das die REST-API aufruft, oder die Kommandozeile. Agenten arbeiten auf denselben Datensätzen und mit denselben Befehlen wie Menschen. Vier Regeln machen das sicher.

1. **Eingeschränkter Umfang.** Ein Agent-Token trägt eine Liste von Berechtigungsgruppen (seine _Scopes_). Es kann die Rechte seines Besitzers nur einschränken, nie erweitern. Die Anfrage nach einer Gruppe, die der Besitzer nicht hat, wird abgelehnt.
2. **Maskiert.** Personenbezogene Daten kommen als stabile Platzhalter wie `EMAIL_1` zurück. Finanzielle, Personal- und besonders schützenswerte Felder werden zurückgehalten. Ein Agent kann trotzdem über Datensätze nachdenken und Platzhalter zurückgeben, sieht aber nie die echten Werte. Siehe [Maskierungs-Tokens](/docs/agents/masking/).
3. **Vorschlagen, nicht ausführen.** Ein Agent kann gewöhnliche Aktionen innerhalb seiner Scopes ausführen, bei allem Unumkehrbaren (eine Rechnung buchen, bezahlen, erstatten, versenden, veröffentlichen, löschen) aber nur etwas _vorschlagen_. Der Server liefert eine Plan-ID zurück und ändert nichts. Eine Person gibt den Plan frei. Siehe [Der Freigabeablauf](/docs/agents/approvals/).
4. **Dieselbe Tür.** REST, MCP und die CLI laufen über einen gemeinsamen Dispatcher, ein Agent hat also genau dieselben Berechtigungen, dieselbe Maskierung und dieselben Freigaben, auf welchem Weg er sich auch verbindet.

## Drei Wege der Verbindung

| Schnittstelle  | Wann sie passt                                                                                                   | Seite                                     |
| -------------- | ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| MCP über stdio | Ein MCP-Client (Claude Desktop, Claude Code, Cursor, VS Code, Windsurf und andere) soll obop als Tools aufrufen. | [Über MCP verbinden](/docs/agents/mcp/)   |
| REST           | Ein Skript oder Dienst ruft HTTP-Endpunkte auf.                                                                  | [Die REST-API nutzen](/docs/agents/rest/) |
| CLI            | Eine Shell oder ein Cron-Job ruft `obop call` auf.                                                               | [Die CLI nutzen](/docs/agents/cli/)       |

## Einrichtung in fünf Schritten

1. Legen Sie fest, was der Agent tun soll und welche Gruppen er dafür braucht. Weniger ist besser.
2. [Erstellen Sie ein Agent-Token](/docs/agents/tokens/) mit genau diesen Scopes.
3. Verbinden Sie den Agenten über [MCP](/docs/agents/mcp/), [REST](/docs/agents/rest/) oder die [CLI](/docs/agents/cli/).
4. Probieren Sie einen harmlosen Lesezugriff aus, dann eine Änderung, die eine Freigabe braucht, und geben Sie sie als Person frei.
5. Lesen Sie [Empfehlungen zur Sicherheit](/docs/agents/safety/), bevor Sie das Token an etwas weitergeben, das unbeaufsichtigt läuft.

## Was es heute gibt und was nicht

- Gebaut: Tokens mit eingeschränktem Umfang, Maskierung und Wiederherstellung, Freigabepläne, MCP über stdio, REST und die CLI.
- Noch nicht gebaut: ein HTTP-Transport (remote) für MCP mit OAuth, weshalb gehostete Instanzen noch nicht über MCP erreichbar sind; eine Oberfläche zum Erstellen oder Widerrufen von Agent-Tokens; eine Liste offener Freigaben in der Oberfläche. Agent-Tokens laufen nicht ab, und einen Befehl zum Widerrufen gibt es noch nicht. Darauf weisen wir erneut hin, wo es relevant ist.

Beispiele für Prompts und Abläufe finden Sie unter [Beispiel-Workflows](/docs/agents/workflows/), bei Problemen unter [Fehlerbehebung](/docs/agents/troubleshooting/).
