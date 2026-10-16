---
title: "Platforma"
description: "Jak działa platforma obop: instancje z jedną bazą danych na firmę, moduły i marketplace, grupy uprawnień, klasy danych i maskowanie, zatwierdzanie oraz interfejsy MCP, REST i CLI."
lead: "Jeden program, jeden model danych, trzy sposoby dostępu. Ta strona wyjaśnia, jak te elementy do siebie pasują."
eyebrow: "Jak to działa"
---

## Jedna instancja na firmę

Każda firma działa we własnej **instancji** z własną bazą danych, własnymi użytkownikami i własnym adresem. W usłudze hostowanej płaszczyzna sterowania tworzy instancję po rejestracji i podaje jej adres; osoba, która się zarejestrowała, zostaje jej pierwszym administratorem. Ten sam program możesz też uruchomić samodzielnie. Zobacz [Instancje](/docs/concepts/instances/).

Magazynem jest SQLite do wypróbowania i PostgreSQL do prawdziwego użytku. Rekordy są na razie przechowywane jako dokumenty JSON; warstwa poleceń sprawdza typy pól, a menedżer schematu jest w planach.

## Moduły to aplikacje

Aplikacja to **moduł**: opis modeli, pól, grup uprawnień i akcji przepływu pracy. Pięć modułów jest wbudowanych (Contacts, Products, CRM, Sales, Invoicing). Dwadzieścia dwa kolejne są dołączone jako pakiety. Aplikacje dodajesz z marketplace'u jednym kliknięciem albo wgrywasz własny plik.

Ponieważ każdy moduł przechodzi przez jeden rejestr, kontakt jest tym samym kontaktem w CRM, Sales i Invoicing, a każdy moduł dostaje to samo API, te same ekrany, uprawnienia i zasady prywatności. Zobacz [Aplikacje i moduły](/docs/concepts/apps-and-modules/) oraz [listę aplikacji](/apps/).

## Marketplace

Marketplace to statyczny host udostępniający plik `index.json` i jeden plik `obop.toml` na pakiet. Wbudowany katalog jest dołączony do programu; instancję możesz też skierować na własny host. Każde pobranie jest walidowane tak samo jak wgranie pliku. Zobacz [Marketplace](/marketplace/).

## Uprawnienia

Grupy uprawnień, takie jak `crm.user` lub `invoicing.manager`, są wskazywane przez modele, pola i akcje. Ludzie należą do grup; tokeny agentów mają podzbiór grup swojego właściciela. Zobacz [Grupy uprawnień](/docs/concepts/permission-groups/).

## Klasy danych i maskowanie

Każde pole deklaruje klasę danych (brak, katalog publiczny, wewnętrzne, poufne kadrowe, szczególna kategoria, finansowe, sekret) oraz to, czy jest daną osobową. Ludzie widzą wartości zgodnie ze swoimi uprawnieniami. Agenci widzą dane osobowe jako stałe tokeny zastępcze, a klasy wrażliwe wcale; mogą przekazywać tokeny z powrotem, aby odwołać się do wartości, nie widząc jej. Zobacz [Klasy danych i maskowanie](/docs/concepts/data-classes-and-masking/).

## Przepływy pracy i zatwierdzanie

Zmiany stanu są nazwanymi akcjami. Akcja może wymagać ściślejszej grupy i może wymagać zatwierdzenia. Agent może tylko zaproponować akcję wymagającą zatwierdzenia: serwer zwraca plan i niczego nie zmienia, dopóki człowiek go nie zatwierdzi. Usuwanie rekordów agenci również mogą tylko proponować. Zobacz [Przepływy pracy i zatwierdzanie](/docs/concepts/workflows-and-approvals/).

## Trzy interfejsy, jeden zestaw poleceń

| Interfejs        | Dla kogo                | Jak                                                                                             |
| ---------------- | ----------------------- | ----------------------------------------------------------------------------------------------- |
| Aplikacja webowa | Ludzie                  | Generowana z opisów modułów: listy, tablice, formularze.                                        |
| REST             | Skrypty, usługi, agenci | `POST /api/{model}/{command}` z tokenem okaziciela. [Opis REST](/docs/reference/rest-api/)      |
| MCP              | Asystenci AI            | `obop mcp` przez stdio, jedno narzędzie na polecenie. [Połączenie przez MCP](/docs/agents/mcp/) |
| CLI              | Powłoki i cron          | `obop call model command key=value`. [Opis CLI](/docs/reference/cli/)                           |

Wszystkie wywołują ten sam dyspozytor, więc sprawdzanie uprawnień, maskowanie i zatwierdzanie są identyczne.

## Czego jeszcze nie ma

Warto o tym wiedzieć, zanim się zdecydujesz: nie ma poziomów kodu, więc moduły nie mogą obliczać pól ani wykonywać skutków ubocznych; nie ma uprawnień na poziomie wierszy; nie ma dziennika audytu, narzędzia do eksportu danych ani kopii zapasowych w usłudze hostowanej. [Plan rozwoju](/roadmap/) wymienia, co jest gotowe, a czego jeszcze nie ma.
