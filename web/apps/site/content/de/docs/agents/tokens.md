---
title: "Ein Agent-Token erstellen"
description: "Erstellen Sie ein Agent-Token von obop mit eingeschränktem Umfang über REST (POST /api/auth/agent-token) oder mit der CLI (obop agent-token). Das Token wird einmal angezeigt und kann die Berechtigungsgruppen seines Besitzers nur einschränken."
lead: "Ein Agent-Token ist ein Bearer-Token, das einer Person gehört und eine Liste von Berechtigungsgruppen trägt. Erstellen Sie pro Agent und pro Aufgabe eines."
---

## Was ein Token ist

- Es gehört der Person, die es erstellt hat (seinem _Besitzer_), und handelt mit einer Teilmenge der Gruppen dieser Person.
- Es wird **einmal** angezeigt. obop speichert nur einen Hash, ein verlorenes Token lässt sich also nicht wiederherstellen; erstellen Sie ein neues.
- Es beginnt mit `obop_`. Behandeln Sie es wie ein Passwort.
- Nur eine Person kann Tokens erstellen. Ein Agent kann kein weiteres Token ausstellen.
- Es läuft nicht ab. Es funktioniert nicht, solange das Konto seines Besitzers deaktiviert ist, und endgültig nicht mehr, sobald der Besitzer ausgeschieden ist (Offboarding). Einen Befehl, um ein einzelnes Token zu widerrufen, gibt es noch nicht (siehe [Sicherheit](/docs/agents/safety/)).

## Die Scopes wählen

Scopes sind [Berechtigungsgruppen](/docs/concepts/permission-groups/). Überlegen Sie, was der Agent tun soll, und geben Sie ihm nur diese Gruppen. Gängige Gruppen der integrierten Apps:

| Gruppe              | Der Agent darf                                                          |
| ------------------- | ----------------------------------------------------------------------- |
| `base.user`         | Contacts lesen und schreiben (personenbezogene Daten bleiben maskiert)  |
| `crm.user`          | Leads lesen und schreiben und durch die Pipeline bewegen                |
| `sales.user`        | Angebote und Aufträge lesen und schreiben                               |
| `invoicing.user`    | Rechnungen lesen und schreiben und Entwürfe stornieren                  |
| `invoicing.manager` | das Buchen einer Rechnung vorschlagen (eine Person gibt weiterhin frei) |
| `product.user`      | Produkte und Preise lesen                                               |

Installierte Pakete bringen eigene Gruppen mit, zum Beispiel `helpdesk.agent`. Die [App-Seiten](/apps/) listen die Gruppen jeder App auf.

## Über REST

Melden Sie sich als Person an und fordern Sie dann ein Token an. Dafür brauchen Sie eine laufende Instanz; die Beispiele nutzen eine lokale auf Port 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

Die Antwort ist `{"token":"obop_..."}`: ein Sitzungs-Token, das 12 Stunden gültig ist. Ist für das Konto die Zwei-Faktor-Anmeldung eingeschaltet, lautet die Antwort stattdessen `{"mfa_required":true,"mfa_token":"..."}`; senden Sie `{"mfa_token":"...","code":"123456"}` mit einem Code aus Ihrer Authenticator-App an `POST /api/auth/login/mfa`, um das Sitzungs-Token zu erhalten. Verwenden Sie das Sitzungs-Token als Bearer-Token, um das Agent-Token zu erstellen:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

`scopes` ist erforderlich, darf nicht leer sein und darf nur Gruppen nennen, die in Ihrem eigenen Konto eingetragen sind; `platform.admin` steht hier nicht für die anderen Gruppen. Das erste Konto einer Instanz, die eine Control Plane angelegt hat, hat nur `platform.admin`; geben Sie ihm also zuerst die Gruppen, die Sie weitergeben wollen, unter _Anpassen, Sicherheit, Personal_. `label` ist freier Text. Bei einer solchen Instanz verwenden Sie deren Adresse statt `127.0.0.1:8080` und melden sich mit Ihrer Registrierungs-E-Mail-Adresse als Benutzernamen an.

## Mit der CLI

Wenn Sie obop selbst betreiben, können Sie ein Token direkt auf der Kommandozeile ausstellen. Der Befehl greift direkt auf die Datenbank zu, führen Sie ihn also dort aus, wo die Datenbank erreichbar ist:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Fügen Sie `--db postgres://...` hinzu (oder setzen Sie `OBOP_DATABASE_URL`), wenn die Instanz nicht die Standarddatenbank `sqlite://obop.db` verwendet.

## Das Token für MCP verwenden

Dasselbe Token verbindet einen MCP-Client mit dem entfernten MCP-Server der Instanz, `https://YOUR-INSTANCE/mcp`: Senden Sie es als `Authorization: Bearer obop_...` (Clients haben dafür ein Feld `headers` oder eine Bearer-Token-Variable). Ein mit der CLI oder über REST erstelltes Token funktioniert für beides; es ist nichts MCP-Spezifisches zu erstellen. Beachten Sie dabei:

- Erstellen Sie pro Client ein eigenes Token und geben Sie ihm nur die Scopes, die dieser Client braucht. Die MCP-Tool-Liste besteht genau aus den Befehlen, die die Scopes erlauben.
- Ein Sitzungs-Token aus der Anmeldung funktioniert ebenfalls, läuft aber nach 12 Stunden ab (ein Agent-Token nicht) und handelt als Sie: Personenbezogene Daten werden nicht maskiert, und freigabepflichtige Befehle laufen ohne Freigabe. Verwenden Sie für einen Agenten ein Agent-Token.
- Das Token muss bei jeder Anfrage gesendet werden, nicht nur bei der ersten; Clients tun das, sobald es in ihrer Konfiguration steht.
- Clients, die eine Browser-Anmeldung (OAuth) unterstützen, brauchen kein eingefügtes Token: Sie verbinden sich per Anmeldung und erhalten ein Token, das eine Stunde gilt und nur auf `/mcp` funktioniert. Mit Claude Desktop, claude.ai oder Claude Code haben wir das noch nicht ausprobiert; siehe [Über MCP verbinden](/docs/agents/mcp/).

## Sicher aufbewahren

- Legen Sie es in einer Umgebungsvariablen oder einem Secret-Store ab, nicht in einem Repository.
- Verwenden Sie lieber ein Token pro Agent und pro Aufgabe, jedes nur mit den Scopes, die dieser Agent braucht.
- Ein Token in der Konfigurationsdatei eines Clients ist für alles lesbar, was diese Datei lesen kann.

## Fehler

| Antwort                                                                            | Bedeutung                                                                                                    |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `400 {"error":"an agent token needs at least one scope"}`                          | `scopes` war leer.                                                                                           |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}`     | Sie haben eine Gruppe angefordert, die Sie nicht haben.                                                      |
| `400 {"error":"only a human user can issue agent tokens"}`                         | Sie haben ein Agent-Token benutzt, um ein weiteres anzufordern.                                              |
| `401`                                                                              | Sitzungs-Token fehlt, ist falsch oder abgelaufen.                                                            |
| `403 {"error":"two-factor authentication must be set up before you can continue"}` | Ihr Konto muss zuerst die Zwei-Faktor-Anmeldung einrichten; bis dahin kann diese Sitzung nur sie einrichten. |

Weiter geht es mit [Über MCP verbinden](/docs/agents/mcp/) oder [Die REST-API nutzen](/docs/agents/rest/).
