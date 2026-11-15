---
title: "Ein Agent-Token erstellen"
description: "Erstellen Sie ein Agent-Token von obop mit eingeschränktem Umfang im Backoffice, über REST (POST /api/auth/agent-token) oder mit der CLI (obop agent-token). Das Token wird einmal angezeigt und kann nur einschränken, was sein Besitzer darf."
lead: "Ein Agent-Token ist ein Bearer-Token, das einer Person gehört und eine Liste von Berechtigungsgruppen trägt. Erstellen Sie pro Agent und pro Aufgabe eines."
---

## Was ein Token ist

- Es gehört der Person, die es erstellt hat (seinem _Besitzer_), und handelt mit einer Teilmenge der Gruppen dieser Person.
- Es wird **einmal** angezeigt. obop speichert nur einen Hash, ein verlorenes Token lässt sich also nicht wiederherstellen; erstellen Sie ein neues.
- Es beginnt mit `obop_`. Behandeln Sie es wie ein Passwort.
- Nur eine Person kann Tokens erstellen. Ein Agent kann kein weiteres Token ausstellen.
- Es läuft nicht ab. Sein Besitzer kann es jederzeit widerrufen (siehe [Auflisten und widerrufen](#auflisten-und-widerrufen)). Es funktioniert außerdem nicht, solange das Konto seines Besitzers deaktiviert ist, und endgültig nicht mehr, sobald der Besitzer ausgeschieden ist (Offboarding).

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

## Im Backoffice

Öffnen Sie _Anpassen, Sicherheit_ und gehen Sie zu **Agent-Tokens**. Geben Sie eine Bezeichnung ein (das Programm oder den Agenten, der das Token verwenden wird), haken Sie die Gruppen an und wählen Sie **Agent-Token erstellen**. Das Token wird einmal angezeigt, mit einer Schaltfläche **Kopieren**; wählen Sie **Fertig**, wenn Sie es gespeichert haben. Der Abschnitt listet auch die von Ihnen erstellten Tokens auf, jedes mit **Widerrufen**.

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

`scopes` ist erforderlich und darf nicht leer sein. Es nennt Gruppen, die in Ihrem eigenen Konto eingetragen sind. Ein Administrator (eine Person, die `platform.admin` hat) darf jede Gruppe nennen, die die Instanz hat, weil diese Gruppe bereits alle erlaubt. Das Token trägt trotzdem nur die Gruppen, die Sie nennen: `*` ist keine Gruppe und wird abgelehnt, und eine Gruppe, die kein installiertes Modul deklariert, wird ebenfalls abgelehnt. `label` darf höchstens 80 Zeichen lang sein und nur eine Zeile umfassen. Bei einer Instanz, die eine Control Plane angelegt hat, verwenden Sie deren Adresse statt `127.0.0.1:8080` und melden sich mit Ihrer Registrierungs-E-Mail-Adresse als Benutzernamen an.

## Mit der CLI

Wenn Sie obop selbst betreiben, können Sie ein Token direkt auf der Kommandozeile ausstellen. Der Befehl greift direkt auf die Datenbank zu, führen Sie ihn also dort aus, wo die Datenbank erreichbar ist:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Fügen Sie `--db postgres://...` hinzu (oder setzen Sie `OBOP_DATABASE_URL`), wenn die Instanz nicht die Standarddatenbank `sqlite://obop.db` verwendet. Die Scopes folgen derselben Regel wie über REST, und zwar für den Benutzer, den Sie nennen. Über die Kommandozeile lassen sich Tokens noch nicht auflisten oder widerrufen.

## Auflisten und widerrufen

Ihre eigenen Agent-Tokens sehen und widerrufen Sie unter _Anpassen, Sicherheit, Agent-Tokens_ oder über REST mit Ihrem Sitzungs-Token:

```bash
curl -s http://127.0.0.1:8080/api/auth/agent-tokens \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

```json
{
  "tokens": [
    {
      "id": "5f0c...",
      "label": "triage-bot",
      "scopes": ["base.user", "crm.user"],
      "created_at": 1794650000
    }
  ],
  "delegable": ["base.user", "crm.user"]
}
```

`tokens` enthält nie ein Token, nur wie jedes heißt und was es darf. `delegable` ist die Liste der Gruppen, die Sie auf ein neues Token setzen dürfen. Um ein Token zu widerrufen, senden Sie eine POST-Anfrage an seine `id`:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-tokens/5f0c.../revoke \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

Das Token funktioniert sofort nicht mehr. Nur der Besitzer kann das tun: Ein Administrator kann die Tokens einer anderen Person nicht einzeln auflisten oder widerrufen (das Deaktivieren oder das Offboarding des Kontos beendet alle). Das Erstellen und das Widerrufen eines Tokens werden beide im Sicherheitsprotokoll festgehalten, ohne das Token.

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
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}`     | Sie haben eine Gruppe angefordert, die nicht in Ihrem Konto steht, und Sie sind kein Administrator.          |
| `400 {"error":"nope.user is not a registered permission group, ..."}`              | Ein Administrator hat eine Gruppe angefordert, die kein installiertes Modul deklariert.                      |
| `400 {"error":"only a human user can issue agent tokens"}`                         | Sie haben ein Agent-Token benutzt, um ein weiteres anzufordern.                                              |
| `401`                                                                              | Sitzungs-Token fehlt, ist falsch oder abgelaufen.                                                            |
| `403 {"error":"two-factor authentication must be set up before you can continue"}` | Ihr Konto muss zuerst die Zwei-Faktor-Anmeldung einrichten; bis dahin kann diese Sitzung nur sie einrichten. |

Weiter geht es mit [Über MCP verbinden](/docs/agents/mcp/) oder [Die REST-API nutzen](/docs/agents/rest/).
