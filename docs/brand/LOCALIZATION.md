# Localising the obop interface

How the backoffice, the sign-up app and the shared components are translated, formatted and tested.
The product-level operating model (country packs, statutory vocabulary, reviewers) is
[../LOCALIZATION.md](../LOCALIZATION.md); this file is the working manual for the interface itself.

Seven languages ship: English (source), German, French, Polish, Finnish, Swedish and Spanish. English is
always inline; every other language is one lazily loaded chunk per package, so only the active language is
downloaded.

## 1 How it works

| Piece | Where |
|---|---|
| Runtime (messages, plurals, rich text, Intl formatters, locale-aware amounts) | `web/packages/ui/src/i18n/` |
| Catalogues | `locales/<lang>/messages.json` + `status.json` in `web/packages/ui`, `web/apps/backoffice`, `web/apps/platform` |
| Tooling (extract, check, status, sync) | `web/packages/ui/scripts/i18n.ts` |
| Server metadata labels (module, model, field, option, action names) | `web/apps/backoffice/src/labels.ts`, `metaLabels.ts`; the texts themselves: `packages/labels/<lang>/<id>.toml` (section 8) |
| Server message patterns | `web/apps/backoffice/src/serverMessage.ts`, `web/apps/platform/src/serverMessage.ts` |

### 1.1 Message ids are the English text

```tsx
t("Save");                                          // plain
t("Delete {noun} “{label}”? This cannot be undone.", { noun, label });
t("{count, plural, one {# record} other {# records}}", { count });   // ICU plural, # is formatted
tx("field", "Stage");                               // a context separates words that translate differently
rich("<kbd>⌘S</kbd> save", {}, { kbd: (c) => <Kbd>{c}</Kbd> });     // tags become elements, never HTML
msg("List");                                        // mark for extraction now, translate later with t(id)
```

Why the English text and not a key: the code reads as the screen reads, a missing translation falls back to
readable English instead of `views.list.title`, the extractor needs no registry, and a copy edit shows up as
a changed id so every locale is flagged. The cost is that rewording English re-opens the string in all
locales, which is what we want. A context (`tx("field", …)`) is stored as `context␄text` (the gettext
separator U+0004), so the catalogue maps one-to-one onto PO files (`msgctxt`, `msgid`) for Weblate (D34).

Syntax is a subset of ICU MessageFormat 1: `{name}`, `{n, number}`, `{n, plural, =0 {…} one {…} few {…} other {…}}`
with `#`, `{x, select, a {…} other {…}}`, and an apostrophe quotes `{` and `}` only (`l'instance` stays as is).
`t()` is typed from the literal: a message with `{field}` requires `{ field }`. Plural and select messages
are checked by `i18n:check` instead.

Rules for writing messages:

- No concatenation. A fragment built from pieces cannot be reordered: write one message with placeholders.
- Put the whole sentence, with its punctuation and its quotation marks, in the message. French needs a
  no-break space before `:` and `?`, German wants „…“.
- Never inflect a data noun. A model name ("Lead") is inserted as given; write sentences that tolerate the
  nominative (`Delete {noun} “{label}”?`). Where a count is involved the generic "record(s)" is used instead
  of the model name in every language except English (`nounFor` in `remove.ts`), because "5 leady" is not
  derivable from "Lead".
- Plurals always go through ICU `plural`, with every category the language uses: Polish `one few many other`,
  Finnish, Swedish, German, English `one other`, French and Spanish `one other` (and `many` only for millions,
  which `other` covers). The check fails a catalogue that lacks a category the language reaches below 1000.

### 1.2 Language, region and persistence

Resolution order: `?lang=de` in the address (tests, shared links; not saved) → the person's saved choice
(`localStorage.obop-lang`) → the instance default (`language` in the instance settings, set by an admin with
"Set instance default" under Customize > Theme > Language & region) → `navigator.languages` → English.
The choice is also written to the cookie `obop_lang` (`Path=/; SameSite=Lax; Max-Age=1 year`) so server-rendered
pages (forum, storefront, status page) pick it up. `<html lang dir>` is set on every change.

Region is separate from language: "Same as language" (the default: the browser's own locale when it matches the
language, else `de-DE`, `fr-FR`…) or one of 14 formats (`en-GB`, `en-US`, `en-IE`, `de-DE`, `de-AT`, `de-CH`,
`fr-FR`, `fr-BE`, `fr-CH`, `pl-PL`, `fi-FI`, `sv-SE`, `es-ES`, `es-MX`). A German interface with Swiss formats
is `de` + `de-CH`. Region affects numbers, money, dates, the first day of the week and sorting; it never
changes a stored value. The instance also has a `currency` (ISO 4217); a record with its own `currency` field
(for example a sale order) shows its amounts in that currency, and totals are never added across currencies
(a list footer shows one subtotal per currency, a chart warns).

Changing the language remounts the app tree (state held in the URL and the session survives; an open dialog
does not). The active catalogue is merged from the shared components' and the app's own chunk.

### 1.3 Numbers, money, dates

All formatting goes through `fmt` (`fmt.money`, `fmt.number`, `fmt.date`, `fmt.dateTime`, `fmt.relative`,
`fmt.list`, `fmt.percent`) which uses `Intl` with the active format locale, and sorting through
`compareText` (`Intl.Collator`, numeric, accent-insensitive; Swedish å ä ö sort after z, Polish ł after l).

Money is an integer in minor units (D9). It is turned into text from a decimal digit string (`minorToDecimal`)
so no floating point touches an amount, and typed text is parsed back by digits (`parseNumber`, `parseMoney`):

| Typed | Locale | Result | Note |
|---|---|---|---|
| `12,50` | de, fr, pl, fi, sv, es | 12.50 | the locale's own decimal mark |
| `12.50` | any | 12.50 | the numpad or a habit; two digits cannot be a group |
| `1.250,00` | de, es | 1250.00 | |
| `1 250,5` (space, NBSP or narrow NBSP) | fr, pl, fi, sv | 1250.50 | all three are group marks |
| `1'250.50` | de-CH | 1250.50 | |
| `1.250` | de | 1250.00 | `.` is the group mark of `de` |
| `1,250` | de | 1250.00, **shown first** | three digits after a mark that is not the locale's group mark: the form shows "Read as 1.250,00 €" before commit |
| `12,505` | de | 12.505,00, **shown first** | three digits after a mark: read as a group, flagged like `1,250` |
| `12,5055` | any | rejected | more decimals than the currency allows, and not a group |
| `１２，５０` | any | 12.50 | full-width digits are NFKC-normalised |

The edit field shows the value in the locale (`12,50`), never grouped. The server still receives `1250`.

### 1.4 Server metadata and server messages

`/api/meta` carries `labels` and `descriptions` (language code to text) on every module, model, field and
action, and `option_labels` (option to language to text) on every select field, each only where something is
translated (D52, docs/MODULES.md section 3.7). `labelFor(obj, lang, fallback)` uses `labels[lang]`, then
`labels[base language]`, then the fallback; `describeFor` does the same for descriptions and falls back to the
English `description`; `optionLabel(value, field)` reads `field.option_labels`. The fallback is the client table:
the contexts `app`, `model`, `field`, `option` and `action` in the backoffice catalogue, keyed by the humanised
English name (`expected_revenue_minor` -> `field␄Expected revenue`). `metaLabels.ts` lists the names so the
extractor sees them. The five built-in modules and the 22 bundled packages now send their own labels in all six
languages (section 8), so the client table is the fallback for them and the only source for a module without
labels. Marketplace listings of packages that are not installed still show the English description.

A text field that holds the keys of a select (the status an effect wrote into a history entry) can name that
select with `shows_option_of` (D52 amendment, docs/MODULES.md section 3.3), and `/api/meta` then carries
`shows_option_of` and, with a condition, `shows_option_when` on it. The backoffice draws such a value with
`optionLabel(value, select)`, the lookup above, so the field needs no language data of its own: the stored
value, filters, sorting, the field's own CSV column and the API keep the key, and an edit input shows it (a record named by such a field is named by the label, also in a ref column). The condition is
evaluated in the browser (`displayCondition.ts`, `shownOption.ts`), on the part of the expression language the
install gate allows.

`serverMessage()` maps the stable patterns of docs/MODULES.md 3.5 and the D47/D48/D49 error shapes (`x.y is
required`, `must be one of: …`, `cannot delete …: still referenced by …`, transition refusals) to translatable
messages. In English it returns the text as received. Unknown text is shown as received. The message of a
keyed assertion needs no mapping: the back office sends the language it shows with every command, as `?lang=`
(which outranks the `obop_lang` cookie, a cookie that can differ from the displayed language) and as
`Accept-Language`, and the server words the refusal in it where the package translated it (D52 amendment
2026-10-28, docs/MODULES.md section 3.8), so it is shown as received. A pseudo-locale sends `en`.

### 1.5 Pseudo-locales

`?lang=en-XA` rewrites every translated string with accented look-alikes and doubles vowels (about 40% longer):
`Save` → `Šááṽéé`. A string that stays plain ASCII is hard-coded. `?lang=ar-XB` is the same text with
`dir="rtl"`, to check the mirror. Neither is offered in the switcher.

### 1.6 What is deliberately English

- The module Guide renders `docs/MODULES.md`; it carries `lang="en"` and, in another language, a note says so.
- The Brand book documents the design rules and carries `lang="en"` with a note; its theme controls are translated.
- Server-originated texts without a known pattern, package descriptions, record data, identifiers (`obop.toml`),
  data classes, and example values the person edits.
- Generated records (the starter dashboard) are written in the language of the person who created them.

## 2 Workflow

```sh
cd web/packages/ui
vp run i18n:extract     # rewrite locales/en/messages.json from the sources (also fails on a message built with +)
vp run i18n:check       # fails when a catalogue lacks a key, has an extra key, or has a mismatched placeholder,
                        # plural category or tag; also on a string identical to English that is not allow-listed
vp run i18n:sync        # add status.json entries for new keys (machine-drafted), drop stale ones
vp run i18n:status      # reviewed / machine-drafted counts per language and package
```

`i18n:check` runs as a unit test too (`vp test` in web/packages/ui), so CI fails on a drifting catalogue.

1. Write the English text in the code, run `i18n:extract`. The check now lists the key as missing everywhere.
2. Add the translation to the six `messages.json` files (keep the keys sorted as the extractor wrote them).
3. Run `i18n:sync`; new strings are `machine-drafted`.
4. A native reviewer reads the string in context (`?lang=de`), fixes it and sets it to `reviewed` in `status.json`.
   Statutory vocabulary (tax, invoicing law) is never machine-drafted for release (docs/LOCALIZATION.md).
5. A string that is legitimately identical to English (a brand, an international word) is listed in
   `web/packages/ui/locales/identical.json` under `*` (all languages) or in `locales/<lang>/identical.json`.

Machine-drafted today: every string of the six languages (written by a language model against the glossary
below, not reviewed by a native speaker). Treat them as good drafts.

## 3 Style per language

| | de | fr | pl | fi | sv | es |
|---|---|---|---|---|---|---|
| Address | formal "Sie"; avoid where possible | "vous" | impersonal or "Twoje" | "sinä", rarely written | "du" | "tú", impersonal where possible |
| Buttons | verb (infinitive or imperative form): Speichern | infinitive: Enregistrer | imperative: Zapisz | imperative: Tallenna | imperative: Spara | infinitive: Guardar |
| Capitals | Nouns capitalised; labels in sentence case | sentence case | sentence case | sentence case, no capitalised nouns | sentence case | sentence case |
| Quotes | „…“ | « … » | „…” | ”…” | ”…” | «…» |
| Space before `: ; ? !` | no | no-break space before `: ; ? !` | no | no | no | no |
| Percent, currency | 12 % , 12,50 € | 12 % , 12,50 € | 12% , 12,50 zł | 12 % , 12,50 € | 12 % , 12,50 kr | 12 % , 12,50 € |
| Ellipsis | … (one character), never ... | same | same | same | same | same |

In a message, write the no-break space as the real character (U+00A0). Currency and percent in messages are
produced by `fmt`, not typed.

## 4 Glossary

One term, one translation, everywhere. Add a row before inventing a second word. "Odoo" is the usual
reference for the ERP vocabulary of each market; where it differs from everyday usage the usual word wins.

| # | English | Deutsch | Français | Polski | Suomi | Svenska | Español |
|---|---|---|---|---|---|---|---|
| 1 | lead | Lead | piste | lead | liidi | lead | oportunidad (cliente potencial in prose) |
| 2 | quote (quotation) | Angebot | devis | oferta | tarjous | offert | presupuesto |
| 3 | sale order | Verkaufsauftrag | commande client | zamówienie sprzedaży | myyntitilaus | försäljningsorder | pedido de venta |
| 4 | invoice | Rechnung | facture | faktura | lasku | faktura | factura |
| 5 | credit note | Gutschrift | avoir | faktura korygująca | hyvityslasku | kreditfaktura | nota de crédito |
| 6 | contact | Kontakt | contact | kontakt | yhteystieto | kontakt | contacto |
| 7 | party (person or organisation) | Geschäftspartner | tiers | kontrahent | osapuoli | part | tercero |
| 8 | customer | Kunde | client | klient | asiakas | kund | cliente |
| 9 | vendor | Lieferant | fournisseur | dostawca | toimittaja | leverantör | proveedor |
| 10 | product | Produkt | produit | produkt | tuote | produkt | producto |
| 11 | price | Preis | prix | cena | hinta | pris | precio |
| 12 | amount | Betrag | montant | kwota | summa | belopp | importe |
| 13 | total | Gesamt | total | razem | yhteensä | totalt | total |
| 14 | tax | Steuer | taxe | podatek | vero | skatt | impuesto |
| 15 | payment | Zahlung | paiement | płatność | maksu | betalning | pago |
| 16 | workflow | Workflow | flux de travail | przepływ pracy | työnkulku | arbetsflöde | flujo de trabajo |
| 17 | status | Status | statut | status | tila | status | estado |
| 18 | stage | Phase | étape | etap | vaihe | fas | etapa |
| 19 | approval | Genehmigung | approbation | zatwierdzenie | hyväksyntä | godkännande | aprobación |
| 20 | draft | Entwurf | brouillon | wersja robocza | luonnos | utkast | borrador |
| 21 | posted (accounting) | gebucht | comptabilisé | zaksięgowany | kirjattu | bokförd | contabilizado |
| 22 | confirmed | bestätigt | confirmé | potwierdzony | vahvistettu | bekräftad | confirmado |
| 23 | cancelled | storniert | annulé | anulowany | peruutettu | annullerad | cancelado |
| 24 | won / lost | gewonnen / verloren | gagné / perdu | wygrany / przegrany | voitettu / hävitty | vunnen / förlorad | ganado / perdido |
| 25 | module | Modul | module | moduł | moduuli | modul | módulo |
| 26 | app | App | application | aplikacja | sovellus | app | aplicación |
| 27 | marketplace | Marktplatz | place de marché | marketplace | moduulikauppa | marknadsplats | mercado |
| 28 | instance | Instanz | instance | instancja | instanssi | instans | instancia |
| 29 | permission group | Berechtigungsgruppe | groupe de droits | grupa uprawnień | käyttöoikeusryhmä | behörighetsgrupp | grupo de permisos |
| 30 | data class | Datenklasse | classe de données | klasa danych | tietoluokka | dataklass | clase de datos |
| 31 | personal data | personenbezogene Daten | données personnelles | dane osobowe | henkilötiedot | personuppgifter | datos personales |
| 32 | masked | maskiert | masqué | zamaskowane | peitetty | maskerad | enmascarado |
| 33 | agent | Agent | agent | agent | agentti | agent | agente |
| 34 | token | Token | jeton | token | tunniste (token) | token | token |
| 35 | record | Datensatz | enregistrement | rekord | tietue | post | registro |
| 36 | field | Feld | champ | pole | kenttä | fält | campo |
| 37 | list | Liste | liste | lista | luettelo | lista | lista |
| 38 | board (kanban) | Kanban | kanban | kanban | kanban | kanban | kanban |
| 39 | pivot | Pivot | tableau croisé | tabela przestawna | pivot-taulukko | pivottabell | tabla dinámica |
| 40 | dashboard | Dashboard | tableau de bord | pulpit | koontinäyttö | instrumentpanel | panel de control |
| 41 | tile | Kachel | tuile | kafelek | ruutu | ruta | tarjeta (mosaico) |
| 42 | filter | Filter | filtre | filtr | suodatin | filter | filtro |
| 43 | favorite (saved search) | Favorit | favori | ulubione | suosikki | favorit | favorito |
| 44 | group by | Gruppieren nach | regrouper par | grupuj według | ryhmittele | gruppera efter | agrupar por |
| 45 | customize | Anpassen | personnaliser | dostosuj | mukauta | anpassa | personalizar |
| 46 | theme | Erscheinungsbild | thème | motyw | teema | tema | tema |
| 47 | sign in / sign out | anmelden / abmelden | se connecter / se déconnecter | zaloguj się / wyloguj się | kirjaudu sisään / kirjaudu ulos | logga in / logga ut | iniciar sesión / cerrar sesión |
| 48 | account | Konto | compte | konto | tili | konto | cuenta |
| 49 | password | Passwort | mot de passe | hasło | salasana | lösenord | contraseña |
| 50 | save / discard / delete | speichern / verwerfen / löschen | enregistrer / annuler les modifications / supprimer | zapisz / odrzuć / usuń | tallenna / hylkää / poista | spara / förkasta / ta bort | guardar / descartar / eliminar |

| 51 | ticket (helpdesk) | Ticket | ticket | zgłoszenie | tiketti | ärende | ticket |
| 52 | warehouse | Lager | entrepôt | magazyn | varasto | lagerställe | almacén |
| 53 | employee | Mitarbeiter | employé | pracownik | työntekijä | anställd | empleado |
| 54 | task | Aufgabe | tâche | zadanie | tehtävä | uppgift | tarea |
| 55 | time entry | Zeiteintrag | saisie de temps | wpis | tuntikirjaus | tidsregistrering | registro de horas |
| 56 | manufacturing order | Fertigungsauftrag | ordre de fabrication | zlecenie produkcyjne | valmistustilaus | tillverkningsorder | orden de fabricación (labelled "Pedido" where the client table is shared) |
| 57 | purchase order | Bestellung | commande | zamówienie zakupu | ostotilaus | inköpsorder | pedido |
| 58 | subscription | Abonnement | abonnement | subskrypcja | kestotilaus | abonnemang | suscripción |
| 59 | journal (accounting) | Journal | journal | dziennik | päiväkirja | verifikationsserie | diario |
| 60 | expense | Ausgabe | dépense | wydatek | kulu | utlägg | gasto |
| 61 | vehicle | Fahrzeug | véhicule | pojazd | ajoneuvo | fordon | vehículo |
| 62 | signature request | Signaturanfrage | demande de signature | wniosek o podpis | allekirjoituspyyntö | signeringsbegäran | solicitud de firma |

Rows 51 to 62 are the concepts the bundled packages added (D52); the package files in `packages/labels` are the
authority for every other model, field and option name.

Notes. German keeps "Lead" (CRM usage; "Verkaufschance" is the opportunity). French "piste" is a lead,
"opportunité" an opportunity. Finnish "liidi" is the common sales word; "tarjous" is both quote and offer.
Swedish "offert" is a quote, "faktura" an invoice (not "räkning"). Spanish uses "factura", "presupuesto" for a
quote, "pedido" for an order. Polish "faktura" is an invoice and "oferta" a quote; "kontrahent" is the legal
word for a counterparty, "klient" and "dostawca" are used where the role is known.

The model "Party" (a person or organisation the business deals with) is the *party*, not the *contact*.

## 5 Library decision

Lingui (the library D34 names) was evaluated and not adopted for the interface yet:

- **Licence and fit.** Lingui is MIT. It works through a Babel/SWC macro or a Vite plugin for the `t` macro and
  an extraction CLI; whether that plugin chain runs under Vite+ 1.0.0 (Rolldown and Oxc) is unverified and a
  Babel pass would add a second transform to every build.
- **Size.** `@lingui/core` plus a runtime ICU parser costs roughly 7 to 10 kB gzipped before catalogues; the
  runtime here is about 6.5 kB gzipped for the core (parser, plurals, Intl formatters, amount parsing, pseudo-locale)
  plus about 1.5 kB of React bindings (measured with a minified library build).
- **Surface.** The interface needs ICU plurals, placeholders, tags and context only; everything else (dates,
  lists, relative time, collation) is `Intl`.

The catalogue format is the migration path: ids are English text, contexts are `msgctxt`, messages are an ICU
MF1 subset, so `locales/*/messages.json` converts to PO for Weblate and to Lingui's format without touching
call sites (`t`, `tx`, `rich` map to `t`, `t` with context, `Trans`). When Weblate arrives (D34), the PO files
become the source of truth and `i18n:extract` becomes a Lingui/`xgettext` step; the check and the status file
carry over. Revisit when the macro chain is verified on Vite+ or when MessageFormat 2 is reviewed (2027-H2).

## 6 Known gaps

- Brand book content and the module Guide are English by design (section 1.6).
- Server messages without a stable pattern and the marketplace listing of a package that is not installed are
  English; `docs/MODULES.md` section 3.5 lists the patterns that are mapped.
- A list the server pages (the default list) is sorted by the database, in its own collation; the browser's
  `compareText` orders the lists, groups, pivots and kanban boards it builds itself.
- Region does not yet pick a time zone or a calendar other than Gregorian; dates are plain accounting dates.
- Right-to-left is verified with the `ar-XB` pseudo-locale only (logical CSS, mirrored chevrons); no Arabic or
  Hebrew catalogue ships. CJK needs `Intl.Segmenter` for search and an IME-aware key handling review.
- Weblate, PO import/export and reviewer sign-off are not wired; the status files are the interim tracker.

## 7 Review first

The translators flagged these for native review (all other strings are equally unreviewed, these are the doubtful ones):

- One key serves several modules, so a generic word was chosen: model `Move` (accounting entry and stock move), `Order`,
  `Entry`, `Item`, option `Paid` (invoices and leave), option `Cash` (payment method and journal kind).
- Lead stages and actions in Spanish agree with "oportunidad" (feminine), unlike the glossary's masculine
  "ganado / perdido".
- Reversal and cancel are separate words in the accounting actions (de "Storno buchen" / "Stornieren", pl
  "Wystornuj" / "Anuluj", sv "Återför" / "Annullera"); confirm with an accountant of each market.
- Short form labels with little context: `Shows`, `Records of`, `Combine by`, `Needs`, `Mode`, `Pair`.
- Sentences that insert a model name use colon frames ("Neu: {model}") to avoid declension; native reviewers may
  prefer a rewrite once the server can send the plural and case forms of model names.
- The filter help text keeps `is empty` and `is set` in English because the dashboard filter parser accepts only
  those words.

## 8 Server labels of modules and packages (D52)

The names and descriptions of the five built-in modules, the admin module and the 22 bundled packages are
translated in `packages/labels/<lang>/<id>.toml` (28 files per language, six languages). A file is the
`[labels.<lang>...]` document of docs/MODULES.md section 3.7; the built-in modules embed theirs with
`Module::translated`, the bundled packages get theirs appended by the catalogue. The slot
`action.<action>.assert.<key>` (the message of an assertion with a key, 400 characters) is in the template and the
completeness check like the others; the bundled packages declare no assertion keys, so no file of them has one.

```sh
obop modules labels crm fi > packages/labels/fi/crm.toml   # the template: every text, English values
obop modules check --complete packages/labels/fi/*.toml    # none missing, none still English, no typo'd key
```

- **Terms.** The glossary of section 4 applies. The labels of fields, models, options and actions reuse the
  client catalogue for the same humanised name, so the server and the client table never disagree for a
  built-in module; descriptions are written fresh. New concepts that appear in the packages follow the same
  rule: one word per concept in a language, in every package.
- **Capitalisation and mood** as in section 3: German nouns are capitalised, buttons are verbs (imperative; infinitive
  in French and Spanish), Finnish and the Romance languages use sentence case.
- **Identical to English.** A value equal to the English source fails the completeness test unless allow-listed
  in `packages/labels/identical.json` (acronyms such as SKU, IBAN, VIN) or `packages/labels/<lang>/identical.json`
  (words that are the same in that language, for example "Diesel").
- **Status.** `packages/labels/<lang>/status.json` maps each file id to `machine-drafted` or `reviewed`. All
  168 files (28 per language) are `machine-drafted`: written by a language model against the glossary and the
  existing catalogues, not read by a native speaker. A reviewer fixes the file and sets it to `reviewed`.
  Statutory vocabulary (the accounting, expenses, HR and invoicing packages) must be reviewed before release.
- **Tests.** The Rust test `every_bundled_package_and_builtin_module_is_translated_in_all_six_languages` fails on any
  missing or untranslated text; `label_files_and_status_files_cover_exactly_the_shipped_modules` keeps the files,
  status and allow-lists consistent; `web/packages/e2e/tests/labels.spec.ts` checks the launcher, sidebar,
  column headers, select options and action buttons of an installed package in German and Finnish.

### 8.1 Review list

Collected from the translators (term, file, doubt); all other texts are equally unreviewed.

- **All languages.** Module descriptions of accounting, dashboards, ecommerce, email_marketing, pos, sign and
  website were shortened to the 400-character limit (details such as the `accounting.approver` group name and the
  widget filter example were dropped); JSON examples in descriptions were reworded because `{ }` are not allowed.
- **Account types and bookkeeping terms** (accounting `account_type` options, journals, moves) are the translators'
  own wording in every language and need an accountant per market. Move is "Verifikation" (sv), "Kirjaus" (fi),
  "Operacja" (pl); the UI client table says otherwise in places ("Tapahtuma" in fi, "Transaktion" in sv).
- **de.** "Auftrag" (ecommerce, pos) against "Bestellung" (purchase); "Shop-Eintrag" for Listing; "Autorenzeile".
- **fr.** "CDI" and "CDD" for contract types are French terms; Belgium and Switzerland say "durée indéterminée /
  déterminée". Tax id is "Taxe" (should read "Identifiant fiscal"). "Intervenant" for worker in timesheets.
  Gender agreement of state options ("Planifiée", "Associée") follows each model.
- **pl.** "Kontrahent" for party and partner; "Folder" against "Katalog"; "Lokalizacja" for stock location; the VAT
  example was localised to 23 % and Polish sample codes; "Pozycja w marketplace" and "Podpis autora" are coined.
- **fi.** Coined compounds: "Kassamyynti", "Ostotilaus", "Valmistustilaus", "Vuoromalli", "Asiakirjamalli",
  "Käyttötunniste", "Moduulikaupan moduuli". "Päätä" for Finish reads oddly for manufacturing and projects.
  The first draft of six files (accounting to documents) was rebuilt from the template: spot-check for stray English.
- **sv.** "Order" for the POS sale (maybe "Försäljning"); "Skatte-ID"; "hen" in one planning description;
  "Författarrad".
- **es.** "Slug" kept; "coste" (Spain) against "costo" (Latin America); "Cajas de tienda" for shop tills in pos;
  "Ficha" for Listing in ecommerce and platform; "Pedido" is used for both manufacturing and purchase orders.
