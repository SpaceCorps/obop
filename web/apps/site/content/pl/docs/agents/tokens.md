---
title: "Utwórz token agenta"
description: "Utwórz token agenta obop o ograniczonym zakresie w back office, przez REST (POST /api/auth/agent-token) lub za pomocą CLI (obop agent-token). Token jest pokazywany raz i może jedynie zawęzić to, co wolno jego właścicielowi."
lead: "Token agenta to token okaziciela, który należy do osoby i zawiera listę grup uprawnień. Utwórz jeden dla każdego agenta i każdego zadania."
---

## Czym jest token

- Należy do osoby, która go utworzyła (jego _właściciela_), i działa z podzbiorem grup tej osoby.
- Jest pokazywany **raz**. obop przechowuje tylko jego skrót, więc zgubionego tokenu nie można odzyskać; utwórz nowy.
- Zaczyna się od `obop_`. Traktuj go jak hasło.
- Tokeny mogą tworzyć tylko ludzie. Agent nie może wygenerować kolejnego tokenu.
- Nie wygasa. Jego właściciel może go unieważnić w dowolnym momencie (zobacz [Wyświetl i unieważnij](#wyswietl-i-uniewaznij)). Nie działa też, dopóki konto jego właściciela jest wyłączone, a na stałe przestaje działać, gdy właściciel zostanie wyłączony z firmy (offboarding).

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

## W back office

Otwórz _Dostosuj, Bezpieczeństwo_ i przejdź do **Tokeny agentów**. Wpisz etykietę (program lub agenta, który będzie używał tokenu), zaznacz grupy i wybierz **Utwórz token agenta**. Token jest pokazywany raz, z przyciskiem **Kopiuj**; gdy go zapiszesz, wybierz **Gotowe**. W tej sekcji widać też utworzone przez Ciebie tokeny, każdy z przyciskiem **Odwołaj**.

## Przez REST

Zaloguj się jako osoba, a następnie poproś o token. Potrzebna jest działająca instancja; przykłady używają lokalnej na porcie 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

Odpowiedź to `{"token":"obop_..."}`: token sesji, który działa 12 godzin. Jeśli konto ma włączone logowanie dwuskładnikowe, odpowiedzią jest zamiast tego `{"mfa_required":true,"mfa_token":"..."}`; wyślij `{"mfa_token":"...","code":"123456"}` z kodem z aplikacji uwierzytelniającej do `POST /api/auth/login/mfa`, aby dostać token sesji. Użyj tokenu sesji jako tokenu okaziciela, aby utworzyć token agenta:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

Pole `scopes` jest wymagane i nie może być puste. Wskazuje grupy przypisane do Twojego własnego konta. Administrator (osoba, która ma grupę `platform.admin`) może wskazać dowolną grupę, którą ma instancja, bo ta grupa już na wszystkie pozwala. Token nadal zawiera tylko grupy, które wskażesz: znak `*` nie jest grupą i jest odrzucany, a grupa, której nie deklaruje żaden zainstalowany moduł, też jest odrzucana. Pole `label` może mieć najwyżej 80 znaków i musi mieścić się w jednym wierszu. W instancji utworzonej przez płaszczyznę sterowania użyj jej adresu zamiast `127.0.0.1:8080`, a jako nazwy użytkownika podaj adres e-mail użyty przy rejestracji.

## Za pomocą CLI

Jeśli hostujesz system samodzielnie, możesz wydać token bezpośrednio z wiersza poleceń. Polecenie łączy się bezpośrednio z bazą danych, więc uruchom je tam, gdzie baza jest osiągalna:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Dodaj `--db postgres://...` (lub ustaw `OBOP_DATABASE_URL`), jeśli instancja nie używa domyślnej bazy `sqlite://obop.db`. Zakresy podlegają tej samej regule co przez REST, dla użytkownika, którego wskażesz. Z wiersza poleceń nie można jeszcze wyświetlać ani unieważniać tokenów.

## Wyświetl i unieważnij

Własne tokeny agentów widzisz i unieważniasz w _Dostosuj, Bezpieczeństwo, Tokeny agentów_ albo przez REST za pomocą tokenu sesji:

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

`tokens` nigdy nie zawiera tokenu, tylko to, jak się który nazywa i co może. `delegable` to lista grup, które możesz nadać nowemu tokenowi. Aby unieważnić token, wyślij POST pod jego `id`:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-tokens/5f0c.../revoke \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

Token przestaje działać natychmiast. Może to zrobić tylko właściciel: administrator nie może wyświetlać ani unieważniać tokenów innej osoby pojedynczo (wyłączenie konta lub offboarding kończy działanie wszystkich). Utworzenie i unieważnienie tokenu trafiają do dziennika bezpieczeństwa, bez samego tokenu.

## Użyj tokenu w MCP

Ten sam token łączy klienta MCP ze zdalnym serwerem MCP instancji, `https://YOUR-INSTANCE/mcp`: wysyłaj go jako `Authorization: Bearer obop_...` (klienci mają do tego pole `headers` lub zmienną z tokenem okaziciela). Token utworzony przez CLI lub REST działa w obu przypadkach; nie trzeba tworzyć niczego specjalnego dla MCP. Pamiętaj o następujących zasadach:

- Utwórz osobny token dla każdego klienta i nadaj mu tylko zakresy, których ten klient potrzebuje. Lista narzędzi MCP to dokładnie te polecenia, na które pozwalają zakresy.
- Token sesji z logowania też działa, ale wygasa po 12 godzinach (token agenta nie wygasa) i działa jako Ty: dane osobowe nie są maskowane, a polecenia wymagające zatwierdzenia wykonują się bez zatwierdzenia. Dla agenta używaj tokenu agenta.
- Token trzeba wysyłać w każdym żądaniu, nie tylko w pierwszym; klienci robią to sami, gdy mają go w konfiguracji.
- Klienci obsługujący logowanie w przeglądarce (OAuth) nie potrzebują wklejanego tokenu: łączą się przez logowanie i dostają token, który działa godzinę i tylko na `/mcp`. Nie wypróbowaliśmy jeszcze tego z Claude Desktop, claude.ai ani Claude Code; zobacz [Połączenie przez MCP](/docs/agents/mcp/).

## Przechowuj go bezpiecznie

- Umieść go w zmiennej środowiskowej lub w magazynie sekretów, a nie w repozytorium.
- Najlepiej utwórz jeden token na agenta i na zadanie, każdy tylko z tymi zakresami, których ten agent potrzebuje.
- Tokeny w pliku konfiguracyjnym klienta może odczytać wszystko, co ma dostęp do tego pliku.

## Błędy

| Odpowiedź                                                                          | Znaczenie                                                                                                             |
| ---------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                          | Pole `scopes` było puste.                                                                                             |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}`     | Poproszono o grupę, której nie ma na Twoim koncie, a nie jesteś administratorem.                                      |
| `400 {"error":"nope.user is not a registered permission group, ..."}`              | Administrator poprosił o grupę, której nie deklaruje żaden zainstalowany moduł.                                       |
| `400 {"error":"only a human user can issue agent tokens"}`                         | Użyto tokenu agenta, aby poprosić o kolejny.                                                                          |
| `401`                                                                              | Brakujący, błędny lub wygasły token sesji.                                                                            |
| `403 {"error":"two-factor authentication must be set up before you can continue"}` | Twoje konto musi najpierw skonfigurować logowanie dwuskładnikowe; do tego czasu ta sesja może tylko je skonfigurować. |

Dalej: [połącz się przez MCP](/docs/agents/mcp/) lub [użyj REST API](/docs/agents/rest/).
