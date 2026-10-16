---
title: "Agenci"
description: "Jak skonfigurować agentów AI do pracy z obop: model agenta, tokeny o ograniczonym zakresie, zamaskowane dane osobowe oraz zatwierdzanie, dzięki któremu człowiek kontroluje wszystko, czego nie da się cofnąć."
lead: "obop traktuje agentów AI jak pełnoprawnych użytkowników, którzy są celowo ograniczeni. Ta strona opisuje model; kolejne pokazują, jak skonfigurować agenta."
---

## Model agenta

**Agent** to wszystko, co działa w obop za pomocą tokenu agenta: klient MCP, taki jak Claude Desktop lub Cursor, skrypt wywołujący REST API albo wiersz poleceń. Agenci pracują na tych samych rekordach i za pomocą tych samych poleceń co ludzie. Bezpieczeństwo zapewniają cztery zasady.

1. **Ograniczony zakres.** Token agenta zawiera listę grup uprawnień (jego _zakresy_). Może tylko zawęzić to, co wolno jego właścicielowi, nigdy tego nie poszerza. Prośba o grupę, której właściciel nie posiada, zostanie odrzucona.
2. **Maskowanie.** Dane osobowe wracają jako stałe tokeny zastępcze, takie jak `EMAIL_1`. Pola finansowe, kadrowe i należące do szczególnych kategorii danych są wstrzymane. Agent nadal może wnioskować o rekordach i przekazywać tokeny z powrotem, ale nigdy nie widzi prawdziwych wartości. Zobacz [Tokeny maskujące](/docs/agents/masking/).
3. **Proponuj, nie zatwierdzaj.** Agent może uruchamiać zwykłe akcje w ramach swoich zakresów, ale w przypadku wszystkiego, czego nie da się cofnąć (księgowanie faktury, płatność, zwrot, wysyłka, publikacja, usunięcie), może jedynie _zaproponować_. Serwer zwraca identyfikator planu i niczego nie zmienia. Zatwierdza człowiek. Zobacz [Pętlę zatwierdzania](/docs/agents/approvals/).
4. **Jedne drzwi.** REST, MCP i CLI przechodzą przez jeden dyspozytor, więc agent ma dokładnie te same uprawnienia, maskowanie i zatwierdzanie niezależnie od sposobu połączenia.

## Trzy sposoby połączenia

| Interfejs       | Użyj, gdy                                                                                                    | Strona                                       |
| --------------- | ------------------------------------------------------------------------------------------------------------ | -------------------------------------------- |
| MCP przez stdio | Klient MCP (Claude Desktop, Claude Code, Cursor, VS Code, Windsurf i inne) ma wywoływać obop jako narzędzia. | [Połączenie przez MCP](/docs/agents/mcp/)    |
| REST            | Skrypt lub usługa wywołuje punkty końcowe HTTP.                                                              | [Korzystanie z REST API](/docs/agents/rest/) |
| CLI             | Powłoka lub zadanie cron wywołuje `obop call`.                                                               | [Korzystanie z CLI](/docs/agents/cli/)       |

## Konfiguracja w pięciu krokach

1. Zdecyduj, co agent ma robić i jakich grup do tego potrzebuje. Im mniej, tym lepiej.
2. [Utwórz token agenta](/docs/agents/tokens/) tylko z tymi zakresami.
3. Połącz agenta przez [MCP](/docs/agents/mcp/), [REST](/docs/agents/rest/) lub [CLI](/docs/agents/cli/).
4. Wypróbuj nieszkodliwy odczyt, potem zmianę wymagającą zatwierdzenia i zatwierdź ją jako człowiek.
5. Przeczytaj [Dobre praktyki bezpieczeństwa](/docs/agents/safety/), zanim przekażesz token czemukolwiek, co działa bez nadzoru.

## Co istnieje dziś, a czego jeszcze nie ma

- Gotowe: tokeny o ograniczonym zakresie, maskowanie i przywracanie, plany zatwierdzania, MCP przez stdio, REST i CLI.
- Jeszcze niegotowe: transport MCP przez HTTP (zdalny) z OAuth, więc instancji hostowanych nie można jeszcze osiągnąć przez MCP; ekran do tworzenia i unieważniania tokenów agentów; lista oczekujących zatwierdzeń w interfejsie. Tokeny agentów nie wygasają i nie ma jeszcze polecenia do ich unieważniania. Przypominamy o tym w miejscach, w których ma to znaczenie.

Gotowe prompty i przepływy znajdziesz w [Przykładowych przepływach pracy](/docs/agents/workflows/), a gdy coś nie działa, zajrzyj do [Rozwiązywania problemów](/docs/agents/troubleshooting/).
