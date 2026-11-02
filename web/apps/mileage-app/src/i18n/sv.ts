import type { Translation } from "./en.ts";

/**
 * The Swedish catalogue (docs/spec/mobile.md 8.9). It has every key of `en.ts`, and for a sentence
 * with a count the forms Swedish has (`one`, `other`), which the type `Translation<"one">` checks
 * when the compiler runs.
 *
 * MACHINE-DRAFTED. A language model wrote these texts against the glossary of docs/spec/mobile.md
 * 8.9; no native speaker has read them. The review by a native speaker (MOHV) is the gate before
 * the Swedish build is shipped to anyone, and `vp run i18n:review` prints the texts next to the
 * English ones, by screen, for that reader.
 *
 * Style: "du", sentence case, buttons in the imperative, no-break space (U+00A0) between a number
 * and its unit, the quotation marks ”…” of Swedish. The words of the screens follow the label table
 * of the module (`packages/labels/sv/mileage.toml`): Resa, Sträcka, Start, Stopp, Kassera, Skicka
 * in, and the six states Pågår, Avslutad, Under granskning, Bekräftad, Avvisad, Kasserad.
 */
export const sv: Translation<"one"> = {
  "common.back": "Tillbaka",
  "words.error": "Fel",
  "words.info": "Observera",
  "words.success": "Klart",
  "words.warning": "Varning",
  "words.danger": "Problem",
  "common.cancel": "Avbryt",
  "common.retry": "Försök igen",
  "common.close": "Stäng",
  "common.loading": "Läser in",
  "common.unknown": "Okänd",
  "common.details": "Detaljer",
  "error.generic": "Något gick fel. Försök igen.",
  "error.sync": "Resorna kunde inte skickas. Telefonen sparar dem och försöker igen.",
  "error.database": "Resorna på den här telefonen kunde inte öppnas.",
  "error.database.hint":
    "Inget har raderats. Försök igen. Så länge resorna inte kan öppnas sparas inga nya positioner. Om platstjänsten fortfarande körs kan du stoppa den här. Om Försök igen fortsätter att misslyckas kan nyckeln till resorna ha gått förlorad, och då går de inte att läsa: vägen ut är Inställningar, Appar, obop Mileage, Lagring, Rensa lagring, vilket raderar de resor som inte har laddats upp.",
  "error.database.hint.ios":
    "Inget har raderats. Försök igen. Om telefonen precis har startat, lås upp den en gång och försök igen. Så länge resorna inte kan öppnas sparas inga nya positioner. Om platstjänsten fortfarande körs kan du stoppa den här. Om Försök igen fortsätter att misslyckas kan nyckeln till resorna ha gått förlorad, och då går de inte att läsa: vägen ut är att ta bort appen från hemskärmen och installera den igen, vilket raderar de resor som inte har laddats upp.",
  "error.database.stop": "Stoppa platstjänsten",
  "error.database.stopped": "Platstjänsten stoppades.",

  "unit.trips.one": "{count} resa",
  "unit.trips.other": "{count} resor",
  "unit.points.one": "{count} punkt",
  "unit.points.other": "{count} punkter",
  "unit.minutes.one": "{count} minut",
  "unit.minutes.other": "{count} minuter",
  "unit.km": "{value} km",
  "unit.m": "{value} m",

  "signin.title": "Logga in",
  "signin.intro":
    "Ange adressen till din obop-instans. Du loggar in på instansens egen sida i webbläsaren; appen ser aldrig ditt lösenord.",
  "signin.address": "Serveradress",
  "signin.address.helper": "Till exempel https://obop.example.com",
  "signin.button": "Logga in med webbläsaren",
  "signin.busy": "Loggar in",
  "signin.back": "Tillbaka till registreringen",
  "signin.error.address": "Det är ingen giltig adress. Använd formen https://obop.example.com.",
  "signin.error.cleartext":
    "Den här versionen ansluter bara över https. Använd en adress som börjar med https.",
  "signin.error.cancelled": "Inloggningen slutfördes inte.",
  "signin.error.denied": "Åtkomsten nekades på inloggningssidan.",
  "signin.error.network": "Det gick inte att nå servern. Kontrollera adressen och anslutningen.",
  "signin.error.server": "Den adressen svarade inte som en obop-instans.",
  "signin.error.failed": "Inloggningen misslyckades. Försök igen.",
  "signin.expired.title": "Din inloggning har upphört",
  "signin.expired.message":
    "Logga in igen för att ladda upp dina resor. Resorna på den här telefonen sparas tills de har laddats upp.",
  "signin.expired.unsure.title": "Din inloggning avslutades på den här telefonen",
  "signin.expired.unsure.message":
    "Instansen svarade inte i tid, så telefonen kan inte avgöra om den förnyade inloggningen. För säkerhets skull avslutades inloggningen. Logga in igen för att ladda upp dina resor. Resorna på den här telefonen sparas tills de har laddats upp.",
  "signin.untold.title": "Instansen har inte fått veta",
  "signin.untold.message":
    "Inloggningen är avslutad på den här telefonen, men instansen kunde inte nås och listar därför fortfarande telefonen som inloggad. Ta bort den i back office under Anpassa, Säkerhet, Enheter, annars upphör den av sig själv när inloggningen går ut.",

  "home.title": "Mileage",
  "home.start": "Start",
  "home.start.hint": "Börjar registrera en resa",
  "home.purpose": "Syfte med resan",
  "home.last": "Senaste resan: {distance}, {state}",
  "home.none": "Inga resor än.",
  "home.open.title": "En resa är fortfarande öppen",
  "home.open.message": "Startade {when}. Stoppa den eller kassera den.",
  "home.open.action": "Öppna resan",
  "home.restored.title": "En säkerhetskopia återställde en resedatabas som inte går att läsa",
  "home.restored.message":
    "Nyckeln till den gamla databasen finns kvar på den telefon där den skapades, så den kunde inte öppnas här och ersattes av en tom. Resor som inte hade laddats upp innan säkerhetskopian gjordes är förlorade. Resor som hade laddats upp finns på servern.",
  "home.restored.action": "Jag förstår",
  "home.waiting.title": "Väntar på uppladdning",
  "home.waiting.message.one":
    "{count} resa finns på den här telefonen men inte på servern än. Den är säker.",
  "home.waiting.message.other":
    "{count} resor finns på den här telefonen men inte på servern än. De är säkra.",
  "home.problem.title": "En resa behöver din uppmärksamhet",
  "home.problem.message": "Servern avvisade ett steg. Öppna resan för att se varför.",
  "home.problem.action": "Öppna resor",
  "home.trips": "Resor",
  "home.account": "Konto",

  "purpose.business": "Tjänsteresa",
  "purpose.private": "Privat",
  "purpose.commute": "Arbetsresa",

  "disclosure.title": "Så använder appen din plats",
  "disclosure.body":
    "Appen samlar in din plats för att räkna ut hur långt du körde. Det sker bara mellan Start och Stopp, och insamlingen fortsätter när skärmen är släckt eller appen ligger i bakgrunden, så att hela resan registreras. En avisering visas medan registreringen pågår. Din rutt sparas krypterad på den här telefonen tills din instans har den, och delas inte med någon annan.",
  "disclosure.body.ios":
    "Appen samlar in din plats för att räkna ut hur långt du körde. Det sker bara mellan Start och Stopp, och insamlingen fortsätter när skärmen är släckt eller appen ligger i bakgrunden, så att hela resan registreras. Systemet visar en platsindikator medan registreringen pågår. Din rutt sparas krypterad på den här telefonen tills din instans har den, och delas inte med någon annan.",
  "disclosure.continue": "Fortsätt",
  "disclosure.decline": "Inte nu",
  "permission.denied.title": "Plats behövs för att registrera en resa",
  "permission.denied.message":
    "Utan åtkomst till din plats kan appen inte mäta resan. Du kan tillåta det när du trycker på Start igen.",
  "permission.blocked.title": "Plats är avstängd för den här appen",
  "permission.blocked.message":
    "Öppna appens inställningar och tillåt plats. Tryck sedan på Start igen.",
  "permission.approximate.title": "Exakt plats behövs",
  "permission.approximate.message":
    "Du tillät bara ungefärlig plats, och en resa kan inte mätas med den. Öppna appens inställningar, tillåt Exakt plats och tryck sedan på Start igen.",
  "permission.services.title": "Plats är avstängd på den här telefonen",
  "permission.services.message": "Slå på plats i telefonens snabbinställningar och försök igen.",
  "permission.services.message.ios":
    "Slå på Platstjänster i appen Inställningar, under Integritet och säkerhet, och försök igen.",
  "permission.settings": "Öppna inställningar",
  "permission.notifications.title": "Aviseringar är avstängda",
  "permission.notifications.message":
    "Android visar då inte att en resa registreras. Registreringen fungerar ändå. Du kan slå på aviseringar i appens inställningar.",
  "guide.title": "Låt appen vara igång",
  "guide.body":
    "Vissa telefoner stoppar appar för att spara batteri. Tvångsstoppa inte appen medan du kör. Om en resa slutar registreras, ställ in batterianvändningen för den här appen på Obegränsad i Androids inställningar.",
  "guide.body.ios":
    "Stäng inte appen från appväxlaren medan du kör: iPhone slutar då registrera. Om en resa slutar registreras, öppna appen och tryck på Starta om registreringen.",
  "guide.start": "Starta resan",

  "recording.title": "Resa pågår",
  "recording.distance":
    "{distance} hittills. Det är telefonens egen mätning; servern avgör sträckan.",
  "recording.points.one": "{count} punkt sparad på den här telefonen",
  "recording.points.other": "{count} punkter sparade på den här telefonen",
  "recording.nofix": "Väntar på den första positionen. Gå ut eller vänta en stund.",
  "recording.stop": "Stopp",
  "recording.stop.title": "Stoppa resan?",
  "recording.stop.message": "Resan slutar här och laddas upp.",
  "recording.stop.failed": "Resan kunde inte stoppas. Försök igen.",
  "recording.stuck.title": "Positionsuppdateringarna körs inte",
  "recording.stuck.message":
    "Resan är öppen men telefonen registrerar den inte. Starta om registreringen.",
  "recording.stuck.action": "Starta om registreringen",
  "recording.nofix.title": "Ingen position har kommit än",
  "recording.nofix.message":
    "Inget togs emot under den första minuten. Kontrollera att plats är påslagen på telefonen och att telefonen har Google Play-tjänster, och starta sedan om registreringen.",
  "recording.nofix.message.ios":
    "Inget togs emot under den första minuten. Kontrollera att Platstjänster är påslagna på telefonen och att appen får använda din plats, och starta sedan om registreringen.",
  "recording.quiet.title": "Ingen ny position på ett tag",
  "recording.quiet.message.one":
    "Senaste positionen är {count} minut gammal. Om du står stilla är det normalt. Om du kör, starta om registreringen.",
  "recording.quiet.message.other":
    "Senaste positionen är {count} minuter gammal. Om du står stilla är det normalt. Om du kör, starta om registreringen.",
  "recording.services.title": "Plats är avstängd på den här telefonen",
  "recording.services.message":
    "Slå på plats i telefonens snabbinställningar och starta sedan om registreringen.",
  "recording.services.message.ios":
    "Slå på Platstjänster i appen Inställningar, under Integritet och säkerhet, och starta sedan om registreringen.",
  "recording.precise.title": "Bara ungefärlig plats är tillåten",
  "recording.precise.message":
    "En resa kräver exakt plats. Tillåt Exakt plats i appens inställningar och starta sedan om registreringen.",
  "recording.permission.title": "Plats är inte tillåten längre",
  "recording.permission.message":
    "Tillåt plats i appens inställningar och starta sedan om registreringen.",
  "recording.signedout.title": "Din inloggning har upphört",
  "recording.signedout.message":
    "Resan fortsätter att registreras på den här telefonen och laddas upp när du har loggat in. Du kan stoppa den här.",
  "recording.signedout.action": "Logga in",
  "recording.full.title": "Resan är full",
  "recording.full.message": "En resa rymmer högst 20 000 punkter. Stoppa den och starta en ny.",
  "recording.keep.title": "Låt appen vara igång",
  "recording.keep.message": "Tvångsstoppa den inte medan du kör.",
  "recording.keep.message.ios": "Stäng den inte från appväxlaren medan du kör.",
  "recording.notification.title": "Registrerar din resa",
  "recording.notification.body": "Mileage sparar din rutt. Öppna appen för att stoppa.",
  "recording.error.start": "Resan kunde inte startas. Försök igen.",

  "announce.recording.started": "Registreringen har startat",
  "announce.recording.stopped":
    "Registreringen har stoppats. Resan är sparad på den här telefonen.",
  "announce.recording.stuck": "Registreringen har oväntat slutat. Starta om registreringen.",
  "announce.saved": "Sparat",

  "trips.title": "Resor",
  "trips.empty": "Inga resor än.",
  "trips.offline":
    "Det gick inte att nå servern. Visar de resor som är sparade på den här telefonen.",
  "trips.refresh": "Uppdatera",
  "trips.refreshing": "Uppdaterar",

  "state.recording": "Pågår",
  "state.uploading": "Laddas upp",
  "state.waiting": "Väntar på uppladdning",
  "state.ended": "Avslutad",
  "state.in_review": "Under granskning",
  "state.confirmed": "Bekräftad",
  "state.rejected": "Avvisad",
  "state.discarded": "Kasserad",
  "state.problem": "Behöver uppmärksamhet",

  "trip.title": "Resa",
  "trip.started": "Startade",
  "trip.ended": "Slutade",
  "trip.distance": "Sträcka",
  "trip.distance.pending": "Servern har inte räknat ut den än",
  "trip.purpose": "Syfte",
  "trip.state": "Status",
  "trip.checks": "Serverns kontroller",
  "trip.check.seq_gap.one": "{count} lucka i numreringen av punkterna",
  "trip.check.seq_gap.other": "{count} luckor i numreringen av punkterna",
  "trip.check.speed_jump.one": "{count} punkt uteslöts på grund av ett omöjligt hopp",
  "trip.check.speed_jump.other": "{count} punkter uteslöts på grund av omöjliga hopp",
  "trip.check.low_accuracy.one": "{count} punkt uteslöts på grund av låg noggrannhet",
  "trip.check.low_accuracy.other": "{count} punkter uteslöts på grund av låg noggrannhet",
  "trip.submit": "Skicka in för granskning",
  "trip.submit.hint": "Skickar resan till en granskare",
  "trip.discard": "Kassera resan",
  "trip.rejected": "En granskare avvisade den här resan: {reason}",
  "trip.waiting": "Resan är inte helt uppladdad än. Den är säker på den här telefonen.",
  "trip.incomplete.title": "Servern saknar fortfarande punkter",
  "trip.incomplete.message":
    "Servern har inte tagit emot alla punkter för den här resan än. Telefonen fortsätter att försöka. Om det inte blir klart, kassera resan och registrera den igen.",
  "trip.problem.title": "Servern avvisade ett steg",
  "trip.problem.hint": "Inget är förlorat. Du kan försöka igen eller kassera resan.",
  "trip.problem.hint.unknown": "Inget är förlorat. Du kan försöka igen.",
  "trip.problem.generic": "Servern avvisade det här steget.",
  "trip.problem.state_conflict": "Resan är i ett läge där servern inte godtar det här steget.",
  "trip.problem.discarded":
    "Servern har kasserat den här resan. Kassera den även här, så tas punkterna på den här telefonen bort.",
  "trip.problem.gone":
    "Servern säger att den inte har den här resan. Kassera den här, så tas punkterna på den här telefonen bort.",
  "trip.problem.unknown":
    "Servern svarade ”hittades inte” och telefonen kan inte avgöra vad det betyder för den här resan. Punkterna finns kvar på telefonen och inget raderas. Försök igen senare eller fråga din administratör.",
  "trip.problem.invalid_response": "Servern svarade på ett sätt som appen inte förstår.",
  "trip.problem.details": "Detaljer från servern: {message}",
  "trip.action.failed": "Det fungerade inte. Försök igen.",
  "trip.notfound": "Resan finns inte kvar på den här telefonen.",
  "discard.title": "Kassera den här resan?",
  "discard.message": "En kasserad resa räknas aldrig. Ange varför.",
  "discard.reason": "Orsak",
  "discard.reason.helper": "Obligatoriskt. En granskare kan läsa den.",
  "discard.confirm": "Kassera",

  "foreign.title": "Resor från en annan inloggning",
  "foreign.message.one":
    "Den här telefonen innehåller fortfarande {count} resa, med en rutt på {points}, som registrerades under en annan inloggning eller på en annan server. Den har inte laddats upp och visas inte, och inget raderas förrän du väljer.",
  "foreign.message.other":
    "Den här telefonen innehåller fortfarande {count} resor, med rutter på sammanlagt {points}, som registrerades under en annan inloggning eller på en annan server. De har inte laddats upp och visas inte, och inget raderas förrän du väljer.",
  "foreign.stopped":
    "En resa från den inloggningen höll fortfarande på att registreras. Registreringen stoppades.",
  "foreign.keep": "Logga ut och behåll dem",
  "foreign.keep.hint":
    "Den som registrerade dem kan logga in på den här telefonen senare och ladda upp dem.",
  "foreign.delete": "Radera dem och fortsätt",
  "foreign.delete.title": "Radera de här resorna?",
  "foreign.delete.message.one":
    "{count} resa, med en rutt på {points}, raderas från den här telefonen. Det som servern inte har än går förlorat och kan inte tas tillbaka.",
  "foreign.delete.message.other":
    "{count} resor, med rutter på sammanlagt {points}, raderas från den här telefonen. Det som servern inte har än går förlorat och kan inte tas tillbaka.",
  "foreign.delete.confirm": "Radera dem",

  "account.title": "Konto",
  "account.server": "Server",
  "account.device": "Den här telefonen",
  "account.device.name": "Enhetens namn",
  "account.device.save": "Spara namnet",
  "account.device.saved": "Sparat",
  "account.device.revoke": "Ta bort den här telefonen",
  "account.device.revoke.title": "Ta bort den här telefonen?",
  "account.device.revoke.message": "Instansen avslutar den här inloggningen och appen loggar ut.",
  "account.device.state.active": "Aktiv",
  "account.device.state.revoked": "Borttagen",
  "account.device.list":
    "Andra telefoner visas i back office, under Anpassa, Säkerhet, Enheter. En app kan bara se sin egen telefon.",
  "account.device.error": "Det gick inte att läsa den här telefonen från servern.",
  "account.leave.error":
    "Det fungerade inte och inget togs bort. Kontrollera anslutningen och försök igen.",
  "account.signout": "Logga ut",
  "account.signout.title": "Logga ut?",
  "account.signout.message": "Du loggar in igen med webbläsaren.",
  "account.leave.pending.title": "Vissa resor har inte laddats upp än",
  "account.leave.pending.message.one":
    "{count} resa, med en rutt på {points}, finns inte helt på servern. Om du behåller den laddas den upp nästa gång du loggar in på den här servern med samma konto. Ingen som loggar in med ett annat konto kan se eller ladda upp den.",
  "account.leave.pending.message.other":
    "{count} resor, med rutter på sammanlagt {points}, finns inte helt på servern. Om du behåller dem laddas de upp nästa gång du loggar in på den här servern med samma konto. Ingen som loggar in med ett annat konto kan se eller ladda upp dem.",
  "account.leave.keep.signout": "Behåll dem och logga ut",
  "account.leave.keep.remove": "Behåll dem och ta bort den här telefonen",
  "account.leave.delete": "Radera dem",
  "account.leave.delete.title": "Radera de här resorna?",
  "account.leave.delete.message.one":
    "{count} resa, med en rutt på {points}, raderas från den här telefonen. Det som servern inte har än går förlorat. Det går inte att ångra.",
  "account.leave.delete.message.other":
    "{count} resor, med rutter på sammanlagt {points}, raderas från den här telefonen. Det som servern inte har än går förlorat. Det går inte att ångra.",
  "account.leave.delete.signout": "Radera dem och logga ut",
  "account.leave.delete.remove": "Radera dem och ta bort den här telefonen",
  "account.storage": "Lagring",
  "account.storage.value": "Krypterad på den här telefonen (SQLCipher {version})",
  "account.version": "Appversion",
  "account.upload": "Uppladdning",
  "account.upload.idle": "Allt är uppladdat",
  "account.upload.pending.one": "{count} resa väntar",
  "account.upload.pending.other": "{count} resor väntar",
  "account.upload.stuck.one": "{count} resa har fastnat. Öppna Resor för att se varför.",
  "account.upload.stuck.other": "{count} resor har fastnat. Öppna Resor för att se varför.",
  "account.upload.offline": "Ingen anslutning, försöker igen",
  "account.upload.limited": "Servern bad om att vänta, försöker igen",
  "account.upload.unavailable": "Servern är upptagen, försöker igen",
  "account.upload.signed_out": "Utloggad",
  "account.upload.now": "Ladda upp nu",

  "language.title": "Språk",
  "language.system": "Följ telefonen",
  "language.note":
    "Appens språk. Appens namn och systemets egna texter, till exempel behörighetsdialogerna, följer telefonens språk.",
};
