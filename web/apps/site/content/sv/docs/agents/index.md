---
title: "AI-agenter"
description: "Så ställer du in AI-agenter som arbetar med obop: agentmodellen, token med begränsad behörighet, maskerade personuppgifter och godkännanden som låter en person bestämma över allt som inte går att ångra."
lead: "obop behandlar AI-agenter som förstklassiga användare, som medvetet är begränsade. Den här sidan förklarar modellen; nästa sidor visar hur du ställer in en agent."
---

## Agentmodellen

En **agent** är allt som agerar i obop med en agenttoken: en MCP-klient som Claude Desktop eller Cursor, ett skript som anropar REST-API:et, eller kommandoraden. Agenter arbetar med samma poster och genom samma kommandon som människor. Fyra regler gör det säkert.

1. **Begränsad.** En agenttoken har en lista med behörighetsgrupper (dess _scope_). Den kan bara snäva in vad ägaren får göra, aldrig vidga det. Om du ber om en grupp som ägaren inte har nekas det.
2. **Maskerad.** Personuppgifter kommer tillbaka som stabila platshållare, till exempel `EMAIL_1`. Finansiella uppgifter, personaluppgifter och särskilda kategorier av uppgifter undanhålls. En agent kan ändå resonera om poster och skicka tillbaka platshållare, men ser aldrig de verkliga värdena. Se [Platshållare vid maskering](/docs/agents/masking/).
3. **Föreslå, genomför inte.** En agent kan köra vanliga åtgärder inom sina scope, men för allt som inte går att ångra (bokföra en faktura, betala, återbetala, skicka, publicera, ta bort) kan den bara _föreslå_. Servern returnerar ett plan-id och ändrar ingenting. En person godkänner. Se [Godkännandeflödet](/docs/agents/approvals/).
4. **Samma ingång.** REST, MCP och CLI går genom en och samma dispatcher, så en agent har exakt samma behörigheter, maskering och godkännanden oavsett hur den ansluter.

## Tre sätt att ansluta

| Gränssnitt     | Använd det när                                                                                                | Sida                                     |
| -------------- | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| MCP över stdio | En MCP-klient (Claude Desktop, Claude Code, Cursor, VS Code, Windsurf och andra) ska anropa obop som verktyg. | [Anslut via MCP](/docs/agents/mcp/)      |
| REST           | Ett skript eller en tjänst anropar HTTP-slutpunkter.                                                          | [Använd REST-API:et](/docs/agents/rest/) |
| CLI            | Ett skal eller ett cron-jobb anropar `obop call`.                                                             | [Använd CLI](/docs/agents/cli/)          |

## Inställning i fem steg

1. Bestäm vad agenten ska göra och vilka grupper det kräver. Färre är bättre.
2. [Skapa en agenttoken](/docs/agents/tokens/) med bara de scopen.
3. Anslut agenten via [MCP](/docs/agents/mcp/), [REST](/docs/agents/rest/) eller [CLI](/docs/agents/cli/).
4. Prova en ofarlig läsning, sedan en ändring som kräver godkännande, och godkänn den som person.
5. Läs [Säkerhetsråd](/docs/agents/safety/) innan du ger token till något som körs utan tillsyn.

## Vad som finns i dag, och vad som inte gör det

- Byggt: token med begränsad behörighet, maskering och återställning, godkännandeplaner, MCP över stdio, REST och CLI.
- Inte byggt ännu: en HTTP-transport (fjärr-MCP) med OAuth, så hostade instanser går ännu inte att nå via MCP; en skärmbild för att skapa eller återkalla agenttoken; en lista över väntande godkännanden i gränssnittet. Agenttoken upphör inte att gälla och det finns inget kommando för att återkalla dem ännu. Det här påpekas igen där det spelar roll.

Se [Exempel på arbetsflöden](/docs/agents/workflows/) för uppmaningar och flöden att utgå från, och [Felsökning](/docs/agents/troubleshooting/) när något inte fungerar.
