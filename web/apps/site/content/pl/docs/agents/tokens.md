---
title: "Utwórz token agenta"
description: "Utwórz token agenta obop o ograniczonym zakresie przez REST (POST /api/auth/agent-token) lub za pomocą CLI (obop agent-token). Token jest pokazywany raz i może jedynie zawęzić grupy uprawnień swojego właściciela."
lead: "Token agenta to token okaziciela, który należy do osoby i zawiera listę grup uprawnień. Utwórz jeden dla każdego agenta i każdego zadania."
---

## Czym jest token

- Należy do osoby, która go utworzyła (jego _właściciela_), i działa z podzbiorem grup tej osoby.
- Jest pokazywany **raz**. obop przechowuje tylko jego skrót, więc zgubionego tokenu nie można odzyskać; utwórz nowy.
- Zaczyna się od `obop_`. Traktuj go jak hasło.
- Tokeny mogą tworzyć tylko ludzie. Agent nie może wygenerować kolejnego tokenu.
- Nie wygasa i przestaje działać dopiero po usunięciu jego właściciela. Nie ma jeszcze polecenia do unieważnienia pojedynczego tokenu (zobacz [Bezpieczeństwo](/docs/agents/safety/)).

## Wybierz zakresy

Zakresy to [grupy uprawnień](/docs/concepts/permission-groups/). Zastanów się, co agent ma robić, i nadaj mu tylko te grupy. Typowe grupy wbudowanych aplikacji:

| Grupa               | Pozwala agentowi                                                     |
| ------------------- | -------------------------------------------------------------------- |
| `base.user`         | odczytywać i zapisywać kontakty (dane osobowe pozostają zamaskowane) |
| `crm.user`          | odczytywać i zapisywać leady oraz przesuwać je w lejku sprzedaży     |
| `sales.user`        | odczytywać i zapisywać oferty i zamówienia                           |
| `invoicing.user`    | odczytywać i zapisywać faktury oraz anulować wersje robocze          |
| `invoicing.manager` | zaproponować zaksięgowanie faktury (zatwierdza nadal człowiek)       |
| `product.user`      | odczytywać produkty i ceny                                           |

Zainstalowane pakiety dodają własne grupy, na przykład `helpdesk.agent`. [Strony aplikacji](/apps/) wymieniają grupy każdej z nich.

## Przez REST

Zaloguj się jako osoba, a następnie poproś o token. Potrzebna jest działająca instancja; przykłady używają lokalnej na porcie 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

Odpowiedź to `{"token":"obop_..."}`: token sesji, który działa 12 godzin. Użyj go jako tokenu okaziciela, aby utworzyć token agenta:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

Pole `scopes` jest wymagane i musi wskazywać co najmniej jedną grupę, którą posiadasz. Pole `label` to dowolny tekst. W instancji hostowanej użyj jej adresu zamiast `127.0.0.1:8080`, a jako nazwy użytkownika podaj adres e-mail użyty przy rejestracji.

## Za pomocą CLI

Jeśli hostujesz system samodzielnie, możesz wydać token bezpośrednio z wiersza poleceń. Polecenie łączy się bezpośrednio z bazą danych, więc uruchom je tam, gdzie baza jest osiągalna:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Dodaj `--db postgres://...` (lub ustaw `OBOP_DATABASE_URL`), jeśli instancja nie używa domyślnej bazy `sqlite://obop.db`.

## Użyj tokenu w MCP

Ten sam token łączy klienta MCP ze zdalnym serwerem MCP instancji, `https://YOUR-INSTANCE/mcp`: wysyłaj go jako `Authorization: Bearer obop_...` (klienci mają do tego pole `headers` lub zmienną z tokenem okaziciela). Token utworzony przez CLI lub REST działa w obu przypadkach; nie trzeba tworzyć niczego specjalnego dla MCP. Pamiętaj o następujących zasadach:

- Utwórz osobny token dla każdego klienta i nadaj mu tylko zakresy, których ten klient potrzebuje. Lista narzędzi MCP to dokładnie te polecenia, na które pozwalają zakresy.
- Token sesji z logowania też działa, ale wygasa po 12 godzinach; token agenta nie wygasa.
- Token trzeba wysyłać w każdym żądaniu, nie tylko w pierwszym; klienci robią to sami, gdy mają go w konfiguracji.
- Logowanie OAuth dla MCP jeszcze nie istnieje, więc klienci, którzy oferują tylko logowanie w przeglądarce, nie mogą się połączyć. Zobacz [Połączenie przez MCP](/docs/agents/mcp/).

## Przechowuj go bezpiecznie

- Umieść go w zmiennej środowiskowej lub w magazynie sekretów, a nie w repozytorium.
- Najlepiej utwórz jeden token na agenta i na zadanie, aby je rozróżniać i móc usunąć jeden bez wpływu na pozostałe.
- Tokeny w pliku konfiguracyjnym klienta może odczytać wszystko, co ma dostęp do tego pliku.

## Błędy

| Odpowiedź                                                                      | Znaczenie                                    |
| ------------------------------------------------------------------------------ | -------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                      | Pole `scopes` było puste.                    |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}` | Poproszono o grupę, której nie posiadasz.    |
| `400 {"error":"only a human user can issue agent tokens"}`                     | Użyto tokenu agenta, aby poprosić o kolejny. |
| `401`                                                                          | Brakujący, błędny lub wygasły token sesji.   |

Dalej: [połącz się przez MCP](/docs/agents/mcp/) lub [użyj REST API](/docs/agents/rest/).
