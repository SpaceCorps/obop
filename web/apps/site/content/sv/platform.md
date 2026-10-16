---
title: "Plattformen"
description: "Så fungerar obop-plattformen: instanser med en databas per företag, moduler och marknadsplatsen, behörighetsgrupper, dataklasser och maskering, godkännanden samt gränssnitten MCP, REST och CLI."
lead: "Ett program, en datamodell, tre vägar in. Den här sidan förklarar hur delarna hänger ihop."
eyebrow: "Så fungerar det"
---

## En instans per företag

Varje företag körs i sin egen **instans** med egen databas, egna användare och egen adress. På den hostade tjänsten skapar ett kontrollplan instansen åt dig efter registreringen och ger dig dess adress; den som registrerade sig blir instansens första administratör. Du kan också köra samma program själv. Se [Instanser](/docs/concepts/instances/).

Lagringen är SQLite för att prova och PostgreSQL för skarp användning. Poster lagras tills vidare som JSON-dokument; kommandolagret kontrollerar fälttyperna, och en schemahanterare är planerad.

## Moduler är appar

En app är en **modul**: en beskrivning av modeller, fält, behörighetsgrupper och arbetsflödesåtgärder. Fem moduler är inbyggda (Contacts, Products, CRM, Sales, Invoicing). Tjugotvå till medföljer som paket. Du lägger till appar från marknadsplatsen med ett klick, eller laddar upp din egen fil.

Eftersom alla moduler går genom ett gemensamt register är en kontakt samma kontakt i CRM, Sales och Invoicing, och varje modul får samma API, skärmbilder, behörigheter och integritetsregler. Se [Appar och moduler](/docs/concepts/apps-and-modules/) och [listan över appar](/apps/).

## Marknadsplats

Marknadsplatsen är en statisk värd som serverar `index.json` och en `obop.toml` per paket. Den medföljande katalogen levereras inuti programmet; du kan också peka en instans mot din egen värd. Varje nedladdning valideras på samma sätt som en uppladdning. Se [Marknadsplats](/marketplace/).

## Behörigheter

Behörighetsgrupper som `crm.user` eller `invoicing.manager` anges av modeller, fält och åtgärder. Personer har grupper; agenttoken har en delmängd av ägarens grupper. Se [Behörighetsgrupper](/docs/concepts/permission-groups/).

## Dataklasser och maskering

Varje fält deklarerar en dataklass (ingen, offentlig katalog, intern, konfidentiell personal, särskild kategori, finansiell, hemlig) och om det är en personuppgift. Människor ser värden enligt sina behörigheter. Agenter ser personuppgifter som stabila platshållare och känsliga klasser inte alls, och kan skicka tillbaka platshållare för att hänvisa till ett värde utan att se det. Se [Dataklasser och maskering](/docs/concepts/data-classes-and-masking/).

## Arbetsflöden och godkännanden

Tillståndsändringar är namngivna åtgärder. En åtgärd kan kräva en strängare grupp och kan kräva godkännande. En agent kan bara föreslå en åtgärd som kräver godkännande: servern returnerar en plan och ändrar ingenting förrän en person godkänner den. Att ta bort poster är också något agenter bara föreslår. Se [Arbetsflöden och godkännanden](/docs/concepts/workflows-and-approvals/).

## Tre gränssnitt, en uppsättning kommandon

| Gränssnitt | För                       | Hur                                                                                         |
| ---------- | ------------------------- | ------------------------------------------------------------------------------------------- |
| Webbapp    | Människor                 | Genereras från modulbeskrivningarna: listor, tavlor, formulär.                              |
| REST       | Skript, tjänster, agenter | `POST /api/{model}/{command}` med en bärartoken. [REST-referens](/docs/reference/rest-api/) |
| MCP        | AI-assistenter            | `obop mcp` över stdio, ett verktyg per kommando. [Anslut via MCP](/docs/agents/mcp/)        |
| CLI        | Skal och cron             | `obop call model command key=value`. [CLI-referens](/docs/reference/cli/)                   |

Alla anropar samma dispatcher, så behörighetskontroller, maskering och godkännanden är identiska.

## Vad det inte är (ännu)

Tänk på det här innan du binder dig: det finns inga kodnivåer, så moduler kan inte beräkna fält eller köra sidoeffekter; det finns inga behörigheter på radnivå; det finns ingen granskningslogg, inget verktyg för dataexport och inga säkerhetskopior för den hostade tjänsten. [Färdplanen](/roadmap/) listar vad som är byggt och vad som inte är det.
