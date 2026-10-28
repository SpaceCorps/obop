//! Localisation of the forum and status chrome: navigation, labels, buttons, errors, dates and
//! numbers. English is the source. User-written content is never translated.
//!
//! To add a string: add a row to [`CATALOGUE`] with seven columns in the order of [`Lang::ALL`]
//! (en, de, fr, pl, fi, sv, es). `{name}` placeholders must appear in every column; a unit test
//! enforces this. To add a language: extend [`Lang`], add a column to every row, a month list to
//! [`MONTHS`] and number rules to [`Lang::group_sep`] / [`Lang::decimal_sep`].

use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    De,
    Fr,
    Pl,
    Fi,
    Sv,
    Es,
}

pub const COOKIE: &str = "obop_lang";

impl Lang {
    pub const ALL: [Lang; 7] = [
        Lang::En,
        Lang::De,
        Lang::Fr,
        Lang::Pl,
        Lang::Fi,
        Lang::Sv,
        Lang::Es,
    ];

    pub fn code(self) -> &'static str {
        ["en", "de", "fr", "pl", "fi", "sv", "es"][self.idx()]
    }

    /// Endonym, for the language switcher.
    pub fn name(self) -> &'static str {
        [
            "English",
            "Deutsch",
            "Français",
            "Polski",
            "Suomi",
            "Svenska",
            "Español",
        ][self.idx()]
    }

    /// Open Graph locale.
    pub fn og_locale(self) -> &'static str {
        [
            "en_US", "de_DE", "fr_FR", "pl_PL", "fi_FI", "sv_SE", "es_ES",
        ][self.idx()]
    }

    fn idx(self) -> usize {
        Lang::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    pub fn from_code(c: &str) -> Option<Lang> {
        let primary = c.trim().split(['-', '_']).next()?.to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }

    fn group_sep(self) -> &'static str {
        match self {
            Lang::En => ",",
            Lang::De | Lang::Es => ".",
            Lang::Fr => "\u{202f}",
            Lang::Pl | Lang::Fi | Lang::Sv => "\u{a0}",
        }
    }

    fn decimal_sep(self) -> &'static str {
        if self == Lang::En { "." } else { "," }
    }
}

/// Where the chosen language came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Query,
    Cookie,
    Header,
    Default,
}

/// `?lang=` beats the `obop_lang` cookie, which beats `Accept-Language`, which beats English.
pub fn negotiate(
    query: Option<&str>,
    cookie: Option<&str>,
    accept_language: Option<&str>,
) -> (Lang, Source) {
    if let Some(l) = query.and_then(Lang::from_code) {
        return (l, Source::Query);
    }
    if let Some(l) = cookie.and_then(Lang::from_code) {
        return (l, Source::Cookie);
    }
    if let Some(h) = accept_language {
        let mut items: Vec<(f32, usize, &str)> = h
            .split(',')
            .enumerate()
            .filter_map(|(i, part)| {
                let mut it = part.split(';');
                let tag = it.next()?.trim();
                let q = it
                    .find_map(|p| {
                        p.trim()
                            .strip_prefix("q=")
                            .and_then(|v| v.trim().parse::<f32>().ok())
                    })
                    .unwrap_or(1.0);
                (q > 0.0 && !tag.is_empty()).then_some((q, i, tag))
            })
            .collect();
        items.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, _, tag) in items {
            if let Some(l) = Lang::from_code(tag) {
                return (l, Source::Header);
            }
        }
    }
    (Lang::En, Source::Default)
}

/// As [`negotiate`], for a caller with no default of its own: the language the request really
/// asked for, or `None` when none of the three sources named one of ours. The REST API uses it to
/// word the message of an assertion (the manifest's own text answers a request that asked for none).
pub fn requested(
    query: Option<&str>,
    cookie: Option<&str>,
    accept_language: Option<&str>,
) -> Option<Lang> {
    match negotiate(query, cookie, accept_language) {
        (_, Source::Default) => None,
        (lang, _) => Some(lang),
    }
}

/// Look up a string. Unknown keys return the key itself so a typo is visible, never a panic.
pub fn t(lang: Lang, key: &str) -> &'static str {
    CATALOGUE
        .iter()
        .find(|(k, _)| *k == key)
        .map_or("?", |(_, cols)| cols[lang.idx()])
}

/// Look up a string and fill `{placeholders}`.
pub fn tf(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(lang, key).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

const MONTHS: [[&str; 12]; 7] = [
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    [
        "Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sep.", "Okt.", "Nov.",
        "Dez.",
    ],
    [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ],
    [
        "sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru",
    ],
    [
        "tammikuuta",
        "helmikuuta",
        "maaliskuuta",
        "huhtikuuta",
        "toukokuuta",
        "kesäkuuta",
        "heinäkuuta",
        "elokuuta",
        "syyskuuta",
        "lokakuuta",
        "marraskuuta",
        "joulukuuta",
    ],
    [
        "jan.", "feb.", "mars", "apr.", "maj", "juni", "juli", "aug.", "sep.", "okt.", "nov.",
        "dec.",
    ],
    [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ],
];

/// `16 Oct 2026`, `16. Okt. 2026`, `16 oct. 2026`, ...
pub fn fmt_date(lang: Lang, ts: u64) -> String {
    let c = util::civil(ts);
    let month = MONTHS[lang.idx()][(c.month - 1) as usize];
    let dot = if matches!(lang, Lang::De | Lang::Fi) {
        "."
    } else {
        ""
    };
    format!("{}{dot} {month} {}", c.day, c.year)
}

/// Date and 24-hour UTC time.
pub fn fmt_datetime(lang: Lang, ts: u64) -> String {
    let c = util::civil(ts);
    format!("{}, {:02}:{:02} UTC", fmt_date(lang, ts), c.hour, c.minute)
}

/// Integer with the language's thousands separator.
pub fn fmt_num(lang: Lang, n: u64) -> String {
    let digits = n.to_string();
    if digits.len() < 4 || (lang == Lang::Es && digits.len() < 5) {
        return digits;
    }
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(lang.group_sep());
        }
        out.push(ch);
    }
    out
}

/// Percentage with at most two decimals, e.g. `99.95%` or `99,95 %`.
pub fn fmt_pct(lang: Lang, pct: f64) -> String {
    let rounded = (pct * 100.0).round() / 100.0;
    let mut s = format!("{rounded:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    let s = s.replace('.', lang.decimal_sep());
    match lang {
        Lang::En | Lang::Pl => format!("{s}%"),
        _ => format!("{s}\u{a0}%"),
    }
}

type Row = (&'static str, [&'static str; 7]);

/// Columns: en, de, fr, pl, fi, sv, es.
pub const CATALOGUE: &[Row] = &[
    // ---- chrome ----
    (
        "skip",
        [
            "Skip to main content",
            "Zum Hauptinhalt springen",
            "Aller au contenu principal",
            "Przejdź do treści",
            "Siirry pääsisältöön",
            "Hoppa till huvudinnehållet",
            "Saltar al contenido principal",
        ],
    ),
    (
        "nav.home",
        [
            "Home",
            "Startseite",
            "Accueil",
            "Strona główna",
            "Etusivu",
            "Startsida",
            "Inicio",
        ],
    ),
    (
        "nav.docs",
        [
            "Docs",
            "Dokumentation",
            "Documentation",
            "Dokumentacja",
            "Ohjeet",
            "Dokumentation",
            "Documentación",
        ],
    ),
    (
        "nav.community",
        [
            "Community",
            "Community",
            "Communauté",
            "Społeczność",
            "Yhteisö",
            "Community",
            "Comunidad",
        ],
    ),
    (
        "nav.status",
        [
            "Status", "Status", "État", "Status", "Tila", "Status", "Estado",
        ],
    ),
    (
        "nav.main",
        [
            "Main navigation",
            "Hauptnavigation",
            "Navigation principale",
            "Nawigacja główna",
            "Päävalikko",
            "Huvudnavigering",
            "Navegación principal",
        ],
    ),
    (
        "nav.search",
        [
            "Search",
            "Suchen",
            "Rechercher",
            "Szukaj",
            "Hae",
            "Sök",
            "Buscar",
        ],
    ),
    (
        "nav.search_ph",
        [
            "Search the community",
            "Community durchsuchen",
            "Rechercher dans la communauté",
            "Szukaj w społeczności",
            "Hae yhteisöstä",
            "Sök i communityn",
            "Buscar en la comunidad",
        ],
    ),
    (
        "nav.login",
        [
            "Log in",
            "Anmelden",
            "Se connecter",
            "Zaloguj się",
            "Kirjaudu sisään",
            "Logga in",
            "Iniciar sesión",
        ],
    ),
    (
        "nav.signup",
        [
            "Sign up",
            "Registrieren",
            "S’inscrire",
            "Załóż konto",
            "Rekisteröidy",
            "Registrera dig",
            "Registrarse",
        ],
    ),
    (
        "nav.logout",
        [
            "Log out",
            "Abmelden",
            "Se déconnecter",
            "Wyloguj się",
            "Kirjaudu ulos",
            "Logga ut",
            "Cerrar sesión",
        ],
    ),
    (
        "nav.language",
        [
            "Language", "Sprache", "Langue", "Język", "Kieli", "Språk", "Idioma",
        ],
    ),
    (
        "nav.mod_log",
        [
            "Moderation log",
            "Moderationsprotokoll",
            "Journal de modération",
            "Dziennik moderacji",
            "Moderointiloki",
            "Moderationslogg",
            "Registro de moderación",
        ],
    ),
    (
        "nav.account",
        [
            "Signed in as {name}",
            "Angemeldet als {name}",
            "Connecté en tant que {name}",
            "Zalogowano jako {name}",
            "Kirjautunut: {name}",
            "Inloggad som {name}",
            "Sesión iniciada como {name}",
        ],
    ),
    (
        "nav.breadcrumb",
        [
            "Breadcrumb",
            "Brotkrümelnavigation",
            "Fil d’Ariane",
            "Ścieżka nawigacji",
            "Navigointipolku",
            "Sökväg",
            "Ruta de navegación",
        ],
    ),
    (
        "footer.text",
        [
            "Open source business software.",
            "Open-Source-Unternehmenssoftware.",
            "Logiciel de gestion open source.",
            "Otwartoźródłowe oprogramowanie dla firm.",
            "Avoimen lähdekoodin yritysohjelmisto.",
            "Företagsprogramvara med öppen källkod.",
            "Software empresarial de código abierto.",
        ],
    ),
    (
        "footer.feed",
        [
            "Atom feed",
            "Atom-Feed",
            "Flux Atom",
            "Kanał Atom",
            "Atom-syöte",
            "Atom-flöde",
            "Canal Atom",
        ],
    ),
    (
        "footer.md",
        [
            "Markdown version",
            "Markdown-Version",
            "Version Markdown",
            "Wersja Markdown",
            "Markdown-versio",
            "Markdown-version",
            "Versión en Markdown",
        ],
    ),
    // ---- categories ----
    (
        "cat.general.name",
        [
            "General",
            "Allgemein",
            "Général",
            "Ogólne",
            "Yleistä",
            "Allmänt",
            "General",
        ],
    ),
    (
        "cat.general.desc",
        [
            "Anything about obop that fits nowhere else.",
            "Alles rund um obop, was sonst nirgends passt.",
            "Tout ce qui concerne obop et ne rentre nulle part ailleurs.",
            "Wszystko o obop, co nie pasuje gdzie indziej.",
            "Kaikki obopiin liittyvä, mikä ei sovi muualle.",
            "Allt om obop som inte passar någon annanstans.",
            "Todo sobre obop que no encaja en otra categoría.",
        ],
    ),
    (
        "cat.help.name",
        ["Help", "Hilfe", "Aide", "Pomoc", "Apua", "Hjälp", "Ayuda"],
    ),
    (
        "cat.help.desc",
        [
            "Ask questions and get answers from the community.",
            "Stellen Sie Fragen und erhalten Sie Antworten aus der Community.",
            "Posez vos questions et obtenez des réponses de la communauté.",
            "Zadawaj pytania i otrzymuj odpowiedzi od społeczności.",
            "Kysy kysymyksiä ja saa vastauksia yhteisöltä.",
            "Ställ frågor och få svar från communityn.",
            "Haz preguntas y recibe respuestas de la comunidad.",
        ],
    ),
    (
        "cat.show-and-tell.name",
        [
            "Show and tell",
            "Zeigen und erzählen",
            "Présentations",
            "Pokaż i opowiedz",
            "Esittele",
            "Visa upp",
            "Muestra y cuenta",
        ],
    ),
    (
        "cat.show-and-tell.desc",
        [
            "Share what you built or how you run your business on obop.",
            "Zeigen Sie, was Sie gebaut haben oder wie Sie Ihr Unternehmen mit obop führen.",
            "Partagez ce que vous avez construit ou comment vous gérez votre entreprise avec obop.",
            "Podziel się tym, co zbudowałeś lub jak prowadzisz firmę z obop.",
            "Jaa, mitä olet rakentanut tai miten pyörität yritystäsi obopilla.",
            "Dela vad du har byggt eller hur du driver ditt företag med obop.",
            "Comparte lo que has creado o cómo diriges tu negocio con obop.",
        ],
    ),
    (
        "cat.modules.name",
        [
            "Modules", "Module", "Modules", "Moduły", "Moduulit", "Moduler", "Módulos",
        ],
    ),
    (
        "cat.modules.desc",
        [
            "Discuss modules, packages and the marketplace.",
            "Module, Pakete und den Marketplace besprechen.",
            "Discutez des modules, des paquets et de la place de marché.",
            "Dyskusje o modułach, pakietach i marketplace.",
            "Keskustele moduuleista, paketeista ja markkinapaikasta.",
            "Diskutera moduler, paket och marknadsplatsen.",
            "Habla sobre módulos, paquetes y el marketplace.",
        ],
    ),
    (
        "cat.agents-and-mcp.name",
        [
            "Agents and MCP",
            "Agenten und MCP",
            "Agents et MCP",
            "Agenci i MCP",
            "Agentit ja MCP",
            "Agenter och MCP",
            "Agentes y MCP",
        ],
    ),
    (
        "cat.agents-and-mcp.desc",
        [
            "AI agents, tokens, the CLI and the MCP server.",
            "KI-Agenten, Token, die CLI und der MCP-Server.",
            "Agents IA, jetons, CLI et serveur MCP.",
            "Agenci AI, tokeny, CLI i serwer MCP.",
            "Tekoälyagentit, tunnukset, komentorivi ja MCP-palvelin.",
            "AI-agenter, token, CLI och MCP-servern.",
            "Agentes de IA, tokens, la CLI y el servidor MCP.",
        ],
    ),
    (
        "cat.self-hosting.name",
        [
            "Self-hosting",
            "Selbst hosten",
            "Auto-hébergement",
            "Własny hosting",
            "Itse ylläpito",
            "Egen drift",
            "Autoalojamiento",
        ],
    ),
    (
        "cat.self-hosting.desc",
        [
            "Run obop on your own servers.",
            "obop auf eigenen Servern betreiben.",
            "Exécutez obop sur vos propres serveurs.",
            "Uruchom obop na własnych serwerach.",
            "Aja obopia omilla palvelimillasi.",
            "Kör obop på dina egna servrar.",
            "Ejecuta obop en tus propios servidores.",
        ],
    ),
    (
        "cat.feature-requests.name",
        [
            "Feature requests",
            "Funktionswünsche",
            "Demandes de fonctionnalités",
            "Propozycje funkcji",
            "Ominaisuustoiveet",
            "Funktionsförslag",
            "Solicitudes de funciones",
        ],
    ),
    (
        "cat.feature-requests.desc",
        [
            "Suggest and discuss what obop should do next.",
            "Vorschläge und Diskussionen, was obop als Nächstes können soll.",
            "Proposez et discutez de ce qu’obop devrait faire ensuite.",
            "Zgłaszaj i omawiaj, co obop powinien robić dalej.",
            "Ehdota ja keskustele, mitä obopin pitäisi tehdä seuraavaksi.",
            "Föreslå och diskutera vad obop ska göra härnäst.",
            "Sugiere y debate qué debería hacer obop a continuación.",
        ],
    ),
    (
        "cat.announcements.name",
        [
            "Announcements",
            "Ankündigungen",
            "Annonces",
            "Ogłoszenia",
            "Tiedotteet",
            "Meddelanden",
            "Anuncios",
        ],
    ),
    (
        "cat.announcements.desc",
        [
            "News and releases from the obop team. Only staff can post here.",
            "Neuigkeiten und Releases vom obop-Team. Nur das Team kann hier schreiben.",
            "Nouvelles et versions de l’équipe obop. Seule l’équipe peut publier ici.",
            "Wiadomości i wydania od zespołu obop. Publikować może tylko zespół.",
            "Uutisia ja julkaisuja obop-tiimiltä. Vain tiimi voi kirjoittaa tänne.",
            "Nyheter och utgåvor från obop-teamet. Bara teamet kan skriva här.",
            "Novedades y versiones del equipo de obop. Solo el equipo puede publicar aquí.",
        ],
    ),
    // ---- forum ----
    (
        "forum.title",
        [
            "obop community",
            "obop-Community",
            "Communauté obop",
            "Społeczność obop",
            "obop-yhteisö",
            "obop-community",
            "Comunidad de obop",
        ],
    ),
    (
        "forum.intro",
        [
            "Questions, ideas and show and tell from people running their business on obop.",
            "Fragen, Ideen und Einblicke von Menschen, die ihr Unternehmen mit obop führen.",
            "Questions, idées et retours d’expérience de personnes qui gèrent leur entreprise avec obop.",
            "Pytania, pomysły i prezentacje od osób prowadzących firmę z obop.",
            "Kysymyksiä, ideoita ja esittelyjä ihmisiltä, jotka pyörittävät yritystään obopilla.",
            "Frågor, idéer och inspel från människor som driver sitt företag med obop.",
            "Preguntas, ideas y experiencias de personas que dirigen su negocio con obop.",
        ],
    ),
    (
        "forum.categories",
        [
            "Categories",
            "Kategorien",
            "Catégories",
            "Kategorie",
            "Aihealueet",
            "Kategorier",
            "Categorías",
        ],
    ),
    (
        "forum.pinned_announcements",
        [
            "Pinned announcements",
            "Angeheftete Ankündigungen",
            "Annonces épinglées",
            "Przypięte ogłoszenia",
            "Kiinnitetyt tiedotteet",
            "Fästa meddelanden",
            "Anuncios fijados",
        ],
    ),
    (
        "forum.recent",
        [
            "Recent threads",
            "Neueste Themen",
            "Discussions récentes",
            "Najnowsze wątki",
            "Viimeisimmät ketjut",
            "Senaste trådar",
            "Hilos recientes",
        ],
    ),
    (
        "forum.threads_n",
        [
            "Threads: {n}",
            "Themen: {n}",
            "Discussions : {n}",
            "Wątki: {n}",
            "Ketjuja: {n}",
            "Trådar: {n}",
            "Hilos: {n}",
        ],
    ),
    (
        "forum.posts_n",
        [
            "Posts: {n}",
            "Beiträge: {n}",
            "Messages : {n}",
            "Wpisy: {n}",
            "Viestejä: {n}",
            "Inlägg: {n}",
            "Mensajes: {n}",
        ],
    ),
    (
        "forum.replies_n",
        [
            "Replies: {n}",
            "Antworten: {n}",
            "Réponses : {n}",
            "Odpowiedzi: {n}",
            "Vastauksia: {n}",
            "Svar: {n}",
            "Respuestas: {n}",
        ],
    ),
    (
        "forum.latest",
        [
            "Latest activity {date}",
            "Letzte Aktivität {date}",
            "Dernière activité {date}",
            "Ostatnia aktywność {date}",
            "Viimeisin toiminta {date}",
            "Senaste aktivitet {date}",
            "Última actividad {date}",
        ],
    ),
    (
        "forum.no_activity",
        [
            "No activity yet",
            "Noch keine Aktivität",
            "Pas encore d’activité",
            "Brak aktywności",
            "Ei vielä toimintaa",
            "Ingen aktivitet än",
            "Aún sin actividad",
        ],
    ),
    (
        "forum.no_threads",
        [
            "No threads yet. Start the first one!",
            "Noch keine Themen. Starten Sie das erste!",
            "Aucune discussion pour l’instant. Lancez la première !",
            "Brak wątków. Rozpocznij pierwszy!",
            "Ei vielä ketjuja. Aloita ensimmäinen!",
            "Inga trådar än. Starta den första!",
            "Aún no hay hilos. ¡Empieza el primero!",
        ],
    ),
    (
        "forum.new_thread",
        [
            "Start a new thread",
            "Neues Thema starten",
            "Lancer une discussion",
            "Rozpocznij nowy wątek",
            "Aloita uusi ketju",
            "Starta en ny tråd",
            "Iniciar un hilo nuevo",
        ],
    ),
    (
        "forum.reply",
        [
            "Reply",
            "Antworten",
            "Répondre",
            "Odpowiedz",
            "Vastaa",
            "Svara",
            "Responder",
        ],
    ),
    (
        "forum.post_reply",
        [
            "Post reply",
            "Antwort senden",
            "Publier la réponse",
            "Opublikuj odpowiedź",
            "Julkaise vastaus",
            "Skicka svar",
            "Publicar respuesta",
        ],
    ),
    (
        "forum.post_thread",
        [
            "Post thread",
            "Thema veröffentlichen",
            "Publier la discussion",
            "Opublikuj wątek",
            "Julkaise ketju",
            "Publicera tråd",
            "Publicar hilo",
        ],
    ),
    (
        "forum.title_label",
        [
            "Title", "Titel", "Titre", "Tytuł", "Otsikko", "Rubrik", "Título",
        ],
    ),
    (
        "forum.body_label",
        [
            "Message",
            "Nachricht",
            "Message",
            "Wiadomość",
            "Viesti",
            "Meddelande",
            "Mensaje",
        ],
    ),
    (
        "forum.md_help",
        [
            "Markdown is supported. Raw HTML is shown as text and images become links.",
            "Markdown wird unterstützt. Rohes HTML wird als Text angezeigt, Bilder werden zu Links.",
            "Markdown est pris en charge. Le HTML brut est affiché comme du texte et les images deviennent des liens.",
            "Obsługiwany jest Markdown. Surowy HTML jest wyświetlany jako tekst, a obrazy stają się linkami.",
            "Markdown on tuettu. HTML näytetään tekstinä ja kuvista tulee linkkejä.",
            "Markdown stöds. Rå HTML visas som text och bilder blir länkar.",
            "Se admite Markdown. El HTML sin procesar se muestra como texto y las imágenes se convierten en enlaces.",
        ],
    ),
    (
        "forum.category",
        [
            "Category",
            "Kategorie",
            "Catégorie",
            "Kategoria",
            "Aihealue",
            "Kategori",
            "Categoría",
        ],
    ),
    (
        "forum.edit",
        [
            "Edit",
            "Bearbeiten",
            "Modifier",
            "Edytuj",
            "Muokkaa",
            "Redigera",
            "Editar",
        ],
    ),
    (
        "forum.save",
        [
            "Save changes",
            "Änderungen speichern",
            "Enregistrer les modifications",
            "Zapisz zmiany",
            "Tallenna muutokset",
            "Spara ändringar",
            "Guardar cambios",
        ],
    ),
    (
        "forum.edit_post",
        [
            "Edit post",
            "Beitrag bearbeiten",
            "Modifier le message",
            "Edytuj wpis",
            "Muokkaa viestiä",
            "Redigera inlägg",
            "Editar mensaje",
        ],
    ),
    (
        "forum.edited",
        [
            "edited",
            "bearbeitet",
            "modifié",
            "edytowano",
            "muokattu",
            "redigerad",
            "editado",
        ],
    ),
    (
        "forum.pinned",
        [
            "Pinned",
            "Angeheftet",
            "Épinglé",
            "Przypięty",
            "Kiinnitetty",
            "Fäst",
            "Fijado",
        ],
    ),
    (
        "forum.locked",
        [
            "Locked",
            "Gesperrt",
            "Verrouillé",
            "Zablokowany",
            "Lukittu",
            "Låst",
            "Bloqueado",
        ],
    ),
    (
        "forum.hidden",
        [
            "Hidden",
            "Ausgeblendet",
            "Masqué",
            "Ukryty",
            "Piilotettu",
            "Dold",
            "Oculto",
        ],
    ),
    (
        "forum.staff",
        [
            "Staff", "Team", "Équipe", "Zespół", "Tiimi", "Team", "Equipo",
        ],
    ),
    (
        "forum.removed",
        [
            "This content was removed by a moderator.",
            "Dieser Inhalt wurde von einem Moderator entfernt.",
            "Ce contenu a été supprimé par un modérateur.",
            "Ta treść została usunięta przez moderatora.",
            "Moderaattori on poistanut tämän sisällön.",
            "Det här innehållet har tagits bort av en moderator.",
            "Un moderador ha eliminado este contenido.",
        ],
    ),
    (
        "forum.removed_title",
        [
            "Removed thread",
            "Entferntes Thema",
            "Discussion supprimée",
            "Usunięty wątek",
            "Poistettu ketju",
            "Borttagen tråd",
            "Hilo eliminado",
        ],
    ),
    (
        "forum.locked_msg",
        [
            "This thread is locked. New replies are disabled.",
            "Dieses Thema ist gesperrt. Neue Antworten sind deaktiviert.",
            "Cette discussion est verrouillée. Les nouvelles réponses sont désactivées.",
            "Ten wątek jest zablokowany. Nowe odpowiedzi są wyłączone.",
            "Tämä ketju on lukittu. Uudet vastaukset on poistettu käytöstä.",
            "Den här tråden är låst. Nya svar är avstängda.",
            "Este hilo está bloqueado. Las respuestas nuevas están desactivadas.",
        ],
    ),
    (
        "forum.login_to_reply",
        [
            "Log in or sign up to reply.",
            "Melden Sie sich an oder registrieren Sie sich, um zu antworten.",
            "Connectez-vous ou inscrivez-vous pour répondre.",
            "Zaloguj się lub załóż konto, aby odpowiedzieć.",
            "Kirjaudu sisään tai rekisteröidy vastataksesi.",
            "Logga in eller registrera dig för att svara.",
            "Inicia sesión o regístrate para responder.",
        ],
    ),
    (
        "forum.login_to_post",
        [
            "Log in or sign up to start a thread.",
            "Melden Sie sich an oder registrieren Sie sich, um ein Thema zu starten.",
            "Connectez-vous ou inscrivez-vous pour lancer une discussion.",
            "Zaloguj się lub załóż konto, aby rozpocząć wątek.",
            "Kirjaudu sisään tai rekisteröidy aloittaaksesi ketjun.",
            "Logga in eller registrera dig för att starta en tråd.",
            "Inicia sesión o regístrate para iniciar un hilo.",
        ],
    ),
    (
        "forum.staff_only",
        [
            "Only staff can start threads in this category.",
            "Nur das Team kann in dieser Kategorie Themen starten.",
            "Seule l’équipe peut lancer des discussions dans cette catégorie.",
            "Tylko zespół może zakładać wątki w tej kategorii.",
            "Vain tiimi voi aloittaa ketjuja tässä aihealueessa.",
            "Bara teamet kan starta trådar i den här kategorin.",
            "Solo el equipo puede iniciar hilos en esta categoría.",
        ],
    ),
    (
        "forum.by",
        [
            "by {name}",
            "von {name}",
            "par {name}",
            "od {name}",
            "kirjoittaja {name}",
            "av {name}",
            "por {name}",
        ],
    ),
    (
        "forum.joined",
        [
            "Joined {date}",
            "Beigetreten am {date}",
            "Inscrit le {date}",
            "Dołączył(a) {date}",
            "Liittyi {date}",
            "Gick med {date}",
            "Se unió el {date}",
        ],
    ),
    (
        "forum.page_of",
        [
            "Page {p} of {n}",
            "Seite {p} von {n}",
            "Page {p} sur {n}",
            "Strona {p} z {n}",
            "Sivu {p}/{n}",
            "Sida {p} av {n}",
            "Página {p} de {n}",
        ],
    ),
    (
        "forum.prev",
        [
            "Previous",
            "Zurück",
            "Précédent",
            "Poprzednia",
            "Edellinen",
            "Föregående",
            "Anterior",
        ],
    ),
    (
        "forum.next",
        [
            "Next",
            "Weiter",
            "Suivant",
            "Następna",
            "Seuraava",
            "Nästa",
            "Siguiente",
        ],
    ),
    (
        "forum.pagination",
        [
            "Pagination",
            "Seitennavigation",
            "Pagination",
            "Stronicowanie",
            "Sivutus",
            "Sidnavigering",
            "Paginación",
        ],
    ),
    (
        "forum.search_title",
        [
            "Search results for “{q}”",
            "Suchergebnisse für „{q}“",
            "Résultats pour « {q} »",
            "Wyniki wyszukiwania dla „{q}”",
            "Hakutulokset haulle ”{q}”",
            "Sökresultat för ”{q}”",
            "Resultados de búsqueda para «{q}»",
        ],
    ),
    (
        "forum.search_h",
        [
            "Search",
            "Suche",
            "Recherche",
            "Wyszukiwanie",
            "Haku",
            "Sök",
            "Búsqueda",
        ],
    ),
    (
        "forum.no_results",
        [
            "Nothing found. Try fewer or different words.",
            "Nichts gefunden. Versuchen Sie weniger oder andere Wörter.",
            "Aucun résultat. Essayez moins de mots ou d’autres mots.",
            "Nic nie znaleziono. Spróbuj mniej lub innych słów.",
            "Ei tuloksia. Kokeile vähemmän tai eri sanoja.",
            "Inget hittades. Försök med färre eller andra ord.",
            "No se encontró nada. Prueba con menos palabras u otras.",
        ],
    ),
    (
        "forum.results_n",
        [
            "Results: {n}",
            "Ergebnisse: {n}",
            "Résultats : {n}",
            "Wyniki: {n}",
            "Tuloksia: {n}",
            "Resultat: {n}",
            "Resultados: {n}",
        ],
    ),
    (
        "forum.profile",
        [
            "Member {name}",
            "Mitglied {name}",
            "Membre {name}",
            "Użytkownik {name}",
            "Jäsen {name}",
            "Medlem {name}",
            "Miembro {name}",
        ],
    ),
    (
        "forum.recent_by",
        [
            "Recent threads by {name}",
            "Neueste Themen von {name}",
            "Discussions récentes de {name}",
            "Najnowsze wątki użytkownika {name}",
            "Käyttäjän {name} viimeisimmät ketjut",
            "Senaste trådar av {name}",
            "Hilos recientes de {name}",
        ],
    ),
    (
        "forum.feed_all",
        [
            "Latest threads (Atom)",
            "Neueste Themen (Atom)",
            "Dernières discussions (Atom)",
            "Najnowsze wątki (Atom)",
            "Uusimmat ketjut (Atom)",
            "Senaste trådar (Atom)",
            "Últimos hilos (Atom)",
        ],
    ),
    (
        "forum.post_n",
        [
            "Post {n}",
            "Beitrag {n}",
            "Message {n}",
            "Wpis {n}",
            "Viesti {n}",
            "Inlägg {n}",
            "Mensaje {n}",
        ],
    ),
    // ---- accounts ----
    (
        "auth.login_title",
        [
            "Log in",
            "Anmelden",
            "Connexion",
            "Logowanie",
            "Kirjautuminen",
            "Logga in",
            "Iniciar sesión",
        ],
    ),
    (
        "auth.signup_title",
        [
            "Create an account",
            "Konto erstellen",
            "Créer un compte",
            "Załóż konto",
            "Luo tili",
            "Skapa ett konto",
            "Crear una cuenta",
        ],
    ),
    (
        "auth.email",
        [
            "Email address",
            "E-Mail-Adresse",
            "Adresse e-mail",
            "Adres e-mail",
            "Sähköpostiosoite",
            "E-postadress",
            "Correo electrónico",
        ],
    ),
    (
        "auth.password",
        [
            "Password",
            "Passwort",
            "Mot de passe",
            "Hasło",
            "Salasana",
            "Lösenord",
            "Contraseña",
        ],
    ),
    (
        "auth.display_name",
        [
            "Display name (optional)",
            "Anzeigename (optional)",
            "Nom affiché (facultatif)",
            "Nazwa wyświetlana (opcjonalnie)",
            "Näyttönimi (valinnainen)",
            "Visningsnamn (valfritt)",
            "Nombre visible (opcional)",
        ],
    ),
    (
        "auth.display_help",
        [
            "3 to 24 letters, digits, hyphens or underscores. Your email address is never shown.",
            "3 bis 24 Buchstaben, Ziffern, Bindestriche oder Unterstriche. Ihre E-Mail-Adresse wird nie angezeigt.",
            "De 3 à 24 lettres, chiffres, tirets ou tirets bas. Votre adresse e-mail n’est jamais affichée.",
            "Od 3 do 24 liter, cyfr, myślników lub podkreśleń. Twój adres e-mail nigdy nie jest wyświetlany.",
            "3–24 kirjainta, numeroa, tavuviivaa tai alaviivaa. Sähköpostiosoitettasi ei koskaan näytetä.",
            "3 till 24 bokstäver, siffror, bindestreck eller understreck. Din e-postadress visas aldrig.",
            "De 3 a 24 letras, números, guiones o guiones bajos. Tu correo electrónico nunca se muestra.",
        ],
    ),
    (
        "auth.password_help",
        [
            "At least 12 characters.",
            "Mindestens 12 Zeichen.",
            "Au moins 12 caractères.",
            "Co najmniej 12 znaków.",
            "Vähintään 12 merkkiä.",
            "Minst 12 tecken.",
            "Al menos 12 caracteres.",
        ],
    ),
    (
        "auth.have_account",
        [
            "Already have an account?",
            "Sie haben schon ein Konto?",
            "Vous avez déjà un compte ?",
            "Masz już konto?",
            "Onko sinulla jo tili?",
            "Har du redan ett konto?",
            "¿Ya tienes una cuenta?",
        ],
    ),
    (
        "auth.no_account",
        [
            "New here?",
            "Neu hier?",
            "Nouveau ici ?",
            "Jesteś tu nowy?",
            "Oletko uusi täällä?",
            "Ny här?",
            "¿Eres nuevo?",
        ],
    ),
    (
        "auth.shared",
        [
            "Your obop control-plane account works here as well.",
            "Ihr obop-Konto funktioniert auch hier.",
            "Votre compte obop fonctionne aussi ici.",
            "Twoje konto obop działa również tutaj.",
            "obop-tilisi toimii myös täällä.",
            "Ditt obop-konto fungerar också här.",
            "Tu cuenta de obop también funciona aquí.",
        ],
    ),
    // ---- moderation ----
    (
        "mod.pin",
        [
            "Pin",
            "Anheften",
            "Épingler",
            "Przypnij",
            "Kiinnitä",
            "Fäst",
            "Fijar",
        ],
    ),
    (
        "mod.unpin",
        [
            "Unpin",
            "Lösen",
            "Désépingler",
            "Odepnij",
            "Irrota",
            "Lossa",
            "Desfijar",
        ],
    ),
    (
        "mod.lock",
        [
            "Lock",
            "Sperren",
            "Verrouiller",
            "Zablokuj",
            "Lukitse",
            "Lås",
            "Bloquear",
        ],
    ),
    (
        "mod.unlock",
        [
            "Unlock",
            "Entsperren",
            "Déverrouiller",
            "Odblokuj",
            "Avaa lukitus",
            "Lås upp",
            "Desbloquear",
        ],
    ),
    (
        "mod.hide",
        [
            "Hide",
            "Ausblenden",
            "Masquer",
            "Ukryj",
            "Piilota",
            "Dölj",
            "Ocultar",
        ],
    ),
    (
        "mod.unhide",
        [
            "Unhide",
            "Einblenden",
            "Afficher",
            "Pokaż",
            "Näytä",
            "Visa",
            "Mostrar",
        ],
    ),
    (
        "mod.delete",
        [
            "Remove permanently",
            "Endgültig entfernen",
            "Supprimer définitivement",
            "Usuń na stałe",
            "Poista pysyvästi",
            "Ta bort permanent",
            "Eliminar definitivamente",
        ],
    ),
    (
        "mod.move",
        [
            "Move to category",
            "In Kategorie verschieben",
            "Déplacer vers la catégorie",
            "Przenieś do kategorii",
            "Siirrä aihealueeseen",
            "Flytta till kategori",
            "Mover a la categoría",
        ],
    ),
    (
        "mod.apply",
        [
            "Apply",
            "Anwenden",
            "Appliquer",
            "Zastosuj",
            "Käytä",
            "Tillämpa",
            "Aplicar",
        ],
    ),
    (
        "mod.panel",
        [
            "Moderation",
            "Moderation",
            "Modération",
            "Moderacja",
            "Moderointi",
            "Moderering",
            "Moderación",
        ],
    ),
    (
        "mod.when",
        ["When", "Wann", "Quand", "Kiedy", "Milloin", "När", "Cuándo"],
    ),
    (
        "mod.who",
        [
            "Moderator",
            "Moderator",
            "Modérateur",
            "Moderator",
            "Moderaattori",
            "Moderator",
            "Moderador",
        ],
    ),
    (
        "mod.action",
        [
            "Action", "Aktion", "Action", "Akcja", "Toiminto", "Åtgärd", "Acción",
        ],
    ),
    (
        "mod.target",
        ["Target", "Ziel", "Cible", "Cel", "Kohde", "Mål", "Destino"],
    ),
    (
        "mod.empty_log",
        [
            "No moderation actions yet.",
            "Noch keine Moderationsaktionen.",
            "Aucune action de modération pour l’instant.",
            "Brak działań moderacyjnych.",
            "Ei vielä moderointitoimia.",
            "Inga modereringsåtgärder än.",
            "Aún no hay acciones de moderación.",
        ],
    ),
    // ---- errors ----
    (
        "err.404_title",
        [
            "Page not found",
            "Seite nicht gefunden",
            "Page introuvable",
            "Nie znaleziono strony",
            "Sivua ei löytynyt",
            "Sidan hittades inte",
            "Página no encontrada",
        ],
    ),
    (
        "err.404_text",
        [
            "We could not find that page. Browse the categories or try the search.",
            "Diese Seite wurde nicht gefunden. Stöbern Sie in den Kategorien oder nutzen Sie die Suche.",
            "Cette page est introuvable. Parcourez les catégories ou utilisez la recherche.",
            "Nie znaleziono tej strony. Przejrzyj kategorie lub skorzystaj z wyszukiwarki.",
            "Sivua ei löytynyt. Selaa aihealueita tai käytä hakua.",
            "Sidan hittades inte. Bläddra bland kategorierna eller använd sökningen.",
            "No encontramos esa página. Explora las categorías o usa la búsqueda.",
        ],
    ),
    (
        "err.410_text",
        [
            "This thread was removed and will not come back.",
            "Dieses Thema wurde entfernt und kommt nicht zurück.",
            "Cette discussion a été supprimée et ne reviendra pas.",
            "Ten wątek został usunięty i nie wróci.",
            "Tämä ketju on poistettu eikä palaa.",
            "Den här tråden har tagits bort och kommer inte tillbaka.",
            "Este hilo se eliminó y no volverá.",
        ],
    ),
    (
        "err.403_title",
        [
            "Not allowed",
            "Nicht erlaubt",
            "Action non autorisée",
            "Brak uprawnień",
            "Ei sallittu",
            "Inte tillåtet",
            "No permitido",
        ],
    ),
    (
        "err.403_text",
        [
            "You are not allowed to do that, or the form has expired. Reload the page and try again.",
            "Das ist nicht erlaubt oder das Formular ist abgelaufen. Laden Sie die Seite neu und versuchen Sie es erneut.",
            "Vous n’y êtes pas autorisé ou le formulaire a expiré. Rechargez la page et réessayez.",
            "Nie masz uprawnień do tej akcji lub formularz wygasł. Odśwież stronę i spróbuj ponownie.",
            "Et saa tehdä tätä tai lomake on vanhentunut. Lataa sivu uudelleen ja yritä uudelleen.",
            "Du har inte behörighet eller så har formuläret gått ut. Ladda om sidan och försök igen.",
            "No tienes permiso para hacerlo o el formulario ha caducado. Recarga la página e inténtalo de nuevo.",
        ],
    ),
    (
        "err.405_title",
        [
            "Method not allowed",
            "Methode nicht erlaubt",
            "Méthode non autorisée",
            "Metoda niedozwolona",
            "Menetelmä ei ole sallittu",
            "Metoden tillåts inte",
            "Método no permitido",
        ],
    ),
    (
        "err.429_title",
        [
            "Too many requests",
            "Zu viele Anfragen",
            "Trop de requêtes",
            "Zbyt wiele żądań",
            "Liikaa pyyntöjä",
            "För många förfrågningar",
            "Demasiadas solicitudes",
        ],
    ),
    (
        "err.429_text",
        [
            "You are going too fast. Please wait a while and try again.",
            "Sie sind zu schnell. Bitte warten Sie einen Moment und versuchen Sie es erneut.",
            "Vous allez trop vite. Patientez un moment puis réessayez.",
            "Działasz zbyt szybko. Poczekaj chwilę i spróbuj ponownie.",
            "Etenet liian nopeasti. Odota hetki ja yritä uudelleen.",
            "Du går för fort fram. Vänta en stund och försök igen.",
            "Vas demasiado rápido. Espera un momento e inténtalo de nuevo.",
        ],
    ),
    (
        "err.400_title",
        [
            "Please check your input",
            "Bitte prüfen Sie Ihre Eingabe",
            "Vérifiez votre saisie",
            "Sprawdź wprowadzone dane",
            "Tarkista syötteesi",
            "Kontrollera din inmatning",
            "Revisa lo que has escrito",
        ],
    ),
    (
        "err.413",
        [
            "That message is too large.",
            "Diese Nachricht ist zu groß.",
            "Ce message est trop volumineux.",
            "Ta wiadomość jest za duża.",
            "Viesti on liian suuri.",
            "Meddelandet är för stort.",
            "El mensaje es demasiado grande.",
        ],
    ),
    (
        "err.500",
        [
            "Something went wrong on our side. Please try again later.",
            "Auf unserer Seite ist etwas schiefgelaufen. Bitte versuchen Sie es später erneut.",
            "Un problème est survenu de notre côté. Réessayez plus tard.",
            "Coś poszło nie tak po naszej stronie. Spróbuj ponownie później.",
            "Jokin meni pieleen meidän päässämme. Yritä myöhemmin uudelleen.",
            "Något gick fel hos oss. Försök igen senare.",
            "Algo salió mal por nuestra parte. Inténtalo de nuevo más tarde.",
        ],
    ),
    (
        "err.login_required",
        [
            "Please log in first.",
            "Bitte melden Sie sich zuerst an.",
            "Veuillez d’abord vous connecter.",
            "Najpierw się zaloguj.",
            "Kirjaudu ensin sisään.",
            "Logga in först.",
            "Inicia sesión primero.",
        ],
    ),
    (
        "err.invalid_login",
        [
            "Invalid email address or password.",
            "Ungültige E-Mail-Adresse oder ungültiges Passwort.",
            "Adresse e-mail ou mot de passe invalide.",
            "Nieprawidłowy adres e-mail lub hasło.",
            "Virheellinen sähköpostiosoite tai salasana.",
            "Ogiltig e-postadress eller lösenord.",
            "Correo electrónico o contraseña no válidos.",
        ],
    ),
    (
        "err.too_many_logins",
        [
            "Too many attempts. Try again in a minute.",
            "Zu viele Versuche. Versuchen Sie es in einer Minute erneut.",
            "Trop de tentatives. Réessayez dans une minute.",
            "Zbyt wiele prób. Spróbuj ponownie za minutę.",
            "Liian monta yritystä. Yritä uudelleen minuutin kuluttua.",
            "För många försök. Försök igen om en minut.",
            "Demasiados intentos. Inténtalo de nuevo en un minuto.",
        ],
    ),
    (
        "err.email_invalid",
        [
            "Enter a valid email address.",
            "Geben Sie eine gültige E-Mail-Adresse ein.",
            "Saisissez une adresse e-mail valide.",
            "Podaj prawidłowy adres e-mail.",
            "Anna kelvollinen sähköpostiosoite.",
            "Ange en giltig e-postadress.",
            "Introduce un correo electrónico válido.",
        ],
    ),
    (
        "err.password_short",
        [
            "The password must be at least 12 characters.",
            "Das Passwort muss mindestens 12 Zeichen lang sein.",
            "Le mot de passe doit comporter au moins 12 caractères.",
            "Hasło musi mieć co najmniej 12 znaków.",
            "Salasanan on oltava vähintään 12 merkkiä.",
            "Lösenordet måste vara minst 12 tecken.",
            "La contraseña debe tener al menos 12 caracteres.",
        ],
    ),
    (
        "err.account_exists",
        [
            "An account with this email address already exists. Log in instead.",
            "Mit dieser E-Mail-Adresse existiert bereits ein Konto. Melden Sie sich stattdessen an.",
            "Un compte existe déjà avec cette adresse e-mail. Connectez-vous plutôt.",
            "Konto z tym adresem e-mail już istnieje. Zaloguj się.",
            "Tällä sähköpostiosoitteella on jo tili. Kirjaudu sisään.",
            "Det finns redan ett konto med den här e-postadressen. Logga in i stället.",
            "Ya existe una cuenta con este correo. Inicia sesión.",
        ],
    ),
    (
        "err.name_invalid",
        [
            "The display name must be 3 to 24 letters, digits, hyphens or underscores and must not imitate staff.",
            "Der Anzeigename muss aus 3 bis 24 Buchstaben, Ziffern, Bindestrichen oder Unterstrichen bestehen und darf das Team nicht nachahmen.",
            "Le nom affiché doit comporter de 3 à 24 lettres, chiffres, tirets ou tirets bas et ne doit pas imiter l’équipe.",
            "Nazwa wyświetlana musi mieć od 3 do 24 liter, cyfr, myślników lub podkreśleń i nie może podszywać się pod zespół.",
            "Näyttönimen on oltava 3–24 kirjainta, numeroa, tavuviivaa tai alaviivaa, eikä se saa esiintyä tiiminä.",
            "Visningsnamnet måste vara 3 till 24 bokstäver, siffror, bindestreck eller understreck och får inte efterlikna teamet.",
            "El nombre visible debe tener de 3 a 24 letras, números, guiones o guiones bajos y no puede suplantar al equipo.",
        ],
    ),
    (
        "err.name_taken",
        [
            "That display name is already taken.",
            "Dieser Anzeigename ist bereits vergeben.",
            "Ce nom affiché est déjà pris.",
            "Ta nazwa wyświetlana jest już zajęta.",
            "Tämä näyttönimi on jo käytössä.",
            "Det visningsnamnet är redan upptaget.",
            "Ese nombre visible ya está en uso.",
        ],
    ),
    (
        "err.title_required",
        [
            "Enter a title.",
            "Geben Sie einen Titel ein.",
            "Saisissez un titre.",
            "Podaj tytuł.",
            "Anna otsikko.",
            "Ange en rubrik.",
            "Introduce un título.",
        ],
    ),
    (
        "err.title_long",
        [
            "The title can be at most 140 characters.",
            "Der Titel darf höchstens 140 Zeichen lang sein.",
            "Le titre ne peut pas dépasser 140 caractères.",
            "Tytuł może mieć najwyżej 140 znaków.",
            "Otsikko saa olla enintään 140 merkkiä.",
            "Rubriken får vara högst 140 tecken.",
            "El título puede tener como máximo 140 caracteres.",
        ],
    ),
    (
        "err.body_required",
        [
            "Write a message.",
            "Schreiben Sie eine Nachricht.",
            "Écrivez un message.",
            "Napisz wiadomość.",
            "Kirjoita viesti.",
            "Skriv ett meddelande.",
            "Escribe un mensaje.",
        ],
    ),
    (
        "err.body_long",
        [
            "The message can be at most 20000 characters.",
            "Die Nachricht darf höchstens 20000 Zeichen lang sein.",
            "Le message ne peut pas dépasser 20000 caractères.",
            "Wiadomość może mieć najwyżej 20000 znaków.",
            "Viesti saa olla enintään 20000 merkkiä.",
            "Meddelandet får vara högst 20000 tecken.",
            "El mensaje puede tener como máximo 20000 caracteres.",
        ],
    ),
    (
        "err.links",
        [
            "New accounts can include at most 2 links per post.",
            "Neue Konten dürfen höchstens 2 Links pro Beitrag enthalten.",
            "Les nouveaux comptes peuvent inclure au plus 2 liens par message.",
            "Nowe konta mogą zawierać najwyżej 2 linki we wpisie.",
            "Uusissa tileissä saa olla enintään 2 linkkiä viestiä kohti.",
            "Nya konton får ha högst 2 länkar per inlägg.",
            "Las cuentas nuevas pueden incluir como máximo 2 enlaces por mensaje.",
        ],
    ),
    (
        "err.rate_posts",
        [
            "You can post at most 10 times per hour.",
            "Sie können höchstens 10 Beiträge pro Stunde schreiben.",
            "Vous pouvez publier au plus 10 messages par heure.",
            "Możesz dodać najwyżej 10 wpisów na godzinę.",
            "Voit kirjoittaa enintään 10 viestiä tunnissa.",
            "Du kan skriva högst 10 inlägg per timme.",
            "Puedes publicar como máximo 10 mensajes por hora.",
        ],
    ),
    (
        "err.rate_threads",
        [
            "You can start at most 3 threads per hour.",
            "Sie können höchstens 3 Themen pro Stunde starten.",
            "Vous pouvez lancer au plus 3 discussions par heure.",
            "Możesz założyć najwyżej 3 wątki na godzinę.",
            "Voit aloittaa enintään 3 ketjua tunnissa.",
            "Du kan starta högst 3 trådar per timme.",
            "Puedes iniciar como máximo 3 hilos por hora.",
        ],
    ),
    (
        "err.edit_window",
        [
            "Posts can only be edited for 15 minutes after they were written.",
            "Beiträge können nur 15 Minuten nach dem Schreiben bearbeitet werden.",
            "Les messages ne peuvent être modifiés que pendant 15 minutes après leur rédaction.",
            "Wpisy można edytować tylko przez 15 minut od napisania.",
            "Viestejä voi muokata vain 15 minuutin ajan kirjoittamisen jälkeen.",
            "Inlägg kan bara redigeras i 15 minuter efter att de skrivits.",
            "Los mensajes solo se pueden editar durante 15 minutos después de escribirlos.",
        ],
    ),
    (
        "err.rejected",
        [
            "Your submission was rejected.",
            "Ihre Eingabe wurde abgelehnt.",
            "Votre envoi a été rejeté.",
            "Twoje zgłoszenie zostało odrzucone.",
            "Lähetyksesi hylättiin.",
            "Din inskickning avvisades.",
            "Tu envío fue rechazado.",
        ],
    ),
    (
        "err.category_unknown",
        [
            "Unknown category.",
            "Unbekannte Kategorie.",
            "Catégorie inconnue.",
            "Nieznana kategoria.",
            "Tuntematon aihealue.",
            "Okänd kategori.",
            "Categoría desconocida.",
        ],
    ),
    // ---- status ----
    (
        "status.title",
        [
            "obop status",
            "obop-Status",
            "État d’obop",
            "Status obop",
            "obopin tila",
            "obop-status",
            "Estado de obop",
        ],
    ),
    (
        "status.intro",
        [
            "Live availability of the obop website, control plane, community and customer instances.",
            "Aktuelle Verfügbarkeit von obop-Website, Control Plane, Community und Kundeninstanzen.",
            "Disponibilité en direct du site obop, du plan de contrôle, de la communauté et des instances clientes.",
            "Aktualna dostępność strony obop, panelu sterowania, społeczności i instancji klientów.",
            "obop-sivuston, hallintatason, yhteisön ja asiakasinstanssien reaaliaikainen saatavuus.",
            "Aktuell tillgänglighet för obops webbplats, kontrollplan, community och kundinstanser.",
            "Disponibilidad en tiempo real del sitio de obop, el plano de control, la comunidad y las instancias de clientes.",
        ],
    ),
    (
        "status.all_ok",
        [
            "All systems operational",
            "Alle Systeme laufen",
            "Tous les systèmes sont opérationnels",
            "Wszystkie systemy działają",
            "Kaikki järjestelmät toimivat",
            "Alla system fungerar",
            "Todos los sistemas operativos",
        ],
    ),
    (
        "status.minor",
        [
            "Some systems are degraded",
            "Einige Systeme sind beeinträchtigt",
            "Certains systèmes sont dégradés",
            "Niektóre systemy działają ze spadkiem wydajności",
            "Joidenkin järjestelmien toiminta on heikentynyt",
            "Vissa system är försämrade",
            "Algunos sistemas están degradados",
        ],
    ),
    (
        "status.major",
        [
            "Partial outage",
            "Teilausfall",
            "Panne partielle",
            "Częściowa awaria",
            "Osittainen häiriö",
            "Delvis avbrott",
            "Interrupción parcial",
        ],
    ),
    (
        "status.critical",
        [
            "Major outage",
            "Größerer Ausfall",
            "Panne majeure",
            "Poważna awaria",
            "Laaja häiriö",
            "Större avbrott",
            "Interrupción grave",
        ],
    ),
    (
        "status.maintenance_now",
        [
            "Scheduled maintenance in progress",
            "Geplante Wartung läuft",
            "Maintenance planifiée en cours",
            "Trwa zaplanowana konserwacja",
            "Ajastettu huolto käynnissä",
            "Planerat underhåll pågår",
            "Mantenimiento programado en curso",
        ],
    ),
    (
        "status.components",
        [
            "Components",
            "Komponenten",
            "Composants",
            "Komponenty",
            "Komponentit",
            "Komponenter",
            "Componentes",
        ],
    ),
    (
        "comp.website",
        [
            "Website and docs",
            "Website und Dokumentation",
            "Site web et documentation",
            "Strona i dokumentacja",
            "Verkkosivusto ja ohjeet",
            "Webbplats och dokumentation",
            "Sitio web y documentación",
        ],
    ),
    (
        "comp.api",
        [
            "Control plane API",
            "Control-Plane-API",
            "API du plan de contrôle",
            "API panelu sterowania",
            "Hallintatason API",
            "API för kontrollplanet",
            "API del plano de control",
        ],
    ),
    (
        "comp.community",
        [
            "Community",
            "Community",
            "Communauté",
            "Społeczność",
            "Yhteisö",
            "Community",
            "Comunidad",
        ],
    ),
    (
        "comp.provisioning",
        [
            "Provisioning",
            "Bereitstellung",
            "Provisionnement",
            "Provisioning",
            "Käyttöönotto",
            "Provisionering",
            "Aprovisionamiento",
        ],
    ),
    (
        "comp.celldb",
        [
            "Cell database",
            "Zellen-Datenbank",
            "Base de données de cellule",
            "Baza danych komórki",
            "Solun tietokanta",
            "Cellens databas",
            "Base de datos de celda",
        ],
    ),
    (
        "comp.instances",
        [
            "Customer instances",
            "Kundeninstanzen",
            "Instances clientes",
            "Instancje klientów",
            "Asiakasinstanssit",
            "Kundinstanser",
            "Instancias de clientes",
        ],
    ),
    (
        "state.operational",
        [
            "Operational",
            "In Betrieb",
            "Opérationnel",
            "Działa",
            "Toiminnassa",
            "Fungerar",
            "Operativo",
        ],
    ),
    (
        "state.degraded",
        [
            "Degraded performance",
            "Eingeschränkte Leistung",
            "Performances dégradées",
            "Obniżona wydajność",
            "Heikentynyt suorituskyky",
            "Försämrad prestanda",
            "Rendimiento degradado",
        ],
    ),
    (
        "state.partial",
        [
            "Partial outage",
            "Teilausfall",
            "Panne partielle",
            "Częściowa awaria",
            "Osittainen häiriö",
            "Delvis avbrott",
            "Interrupción parcial",
        ],
    ),
    (
        "state.major",
        [
            "Major outage",
            "Größerer Ausfall",
            "Panne majeure",
            "Poważna awaria",
            "Laaja häiriö",
            "Större avbrott",
            "Interrupción grave",
        ],
    ),
    (
        "state.maintenance",
        [
            "Under maintenance",
            "In Wartung",
            "En maintenance",
            "W konserwacji",
            "Huollossa",
            "Under underhåll",
            "En mantenimiento",
        ],
    ),
    (
        "state.nodata",
        [
            "No data",
            "Keine Daten",
            "Aucune donnée",
            "Brak danych",
            "Ei tietoja",
            "Inga data",
            "Sin datos",
        ],
    ),
    (
        "status.uptime_24h",
        [
            "24 hours",
            "24 Stunden",
            "24 heures",
            "24 godziny",
            "24 tuntia",
            "24 timmar",
            "24 horas",
        ],
    ),
    (
        "status.uptime_7d",
        [
            "7 days",
            "7 Tage",
            "7 jours",
            "7 dni",
            "7 päivää",
            "7 dagar",
            "7 días",
        ],
    ),
    (
        "status.uptime_90d",
        [
            "90 days",
            "90 Tage",
            "90 jours",
            "90 dni",
            "90 päivää",
            "90 dagar",
            "90 días",
        ],
    ),
    (
        "status.uptime",
        [
            "Uptime",
            "Verfügbarkeit",
            "Disponibilité",
            "Dostępność",
            "Käytettävyys",
            "Drifttid",
            "Disponibilidad",
        ],
    ),
    (
        "status.bar_label",
        [
            "Daily availability over the last 90 days, oldest first",
            "Tägliche Verfügbarkeit der letzten 90 Tage, älteste zuerst",
            "Disponibilité quotidienne sur les 90 derniers jours, du plus ancien au plus récent",
            "Dzienna dostępność z ostatnich 90 dni, od najstarszego",
            "Päivittäinen saatavuus viimeisen 90 päivän ajalta, vanhin ensin",
            "Daglig tillgänglighet de senaste 90 dagarna, äldst först",
            "Disponibilidad diaria de los últimos 90 días, del más antiguo al más reciente",
        ],
    ),
    (
        "status.day_label",
        [
            "{date}: {state}, {pct} available",
            "{date}: {state}, {pct} verfügbar",
            "{date} : {state}, {pct} disponible",
            "{date}: {state}, dostępność {pct}",
            "{date}: {state}, saatavuus {pct}",
            "{date}: {state}, {pct} tillgänglig",
            "{date}: {state}, {pct} disponible",
        ],
    ),
    (
        "status.days_ago_90",
        [
            "90 days ago",
            "Vor 90 Tagen",
            "Il y a 90 jours",
            "90 dni temu",
            "90 päivää sitten",
            "För 90 dagar sedan",
            "Hace 90 días",
        ],
    ),
    (
        "status.today",
        [
            "Today",
            "Heute",
            "Aujourd’hui",
            "Dzisiaj",
            "Tänään",
            "Idag",
            "Hoy",
        ],
    ),
    (
        "status.active",
        [
            "Active incidents",
            "Aktuelle Störungen",
            "Incidents en cours",
            "Aktywne incydenty",
            "Käynnissä olevat häiriöt",
            "Pågående incidenter",
            "Incidentes activos",
        ],
    ),
    (
        "status.none_active",
        [
            "No active incidents.",
            "Keine aktuellen Störungen.",
            "Aucun incident en cours.",
            "Brak aktywnych incydentów.",
            "Ei käynnissä olevia häiriöitä.",
            "Inga pågående incidenter.",
            "No hay incidentes activos.",
        ],
    ),
    (
        "status.past",
        [
            "Past incidents",
            "Frühere Störungen",
            "Incidents passés",
            "Wcześniejsze incydenty",
            "Aiemmat häiriöt",
            "Tidigare incidenter",
            "Incidentes anteriores",
        ],
    ),
    (
        "status.none_past",
        [
            "No incidents in the last 90 days.",
            "Keine Störungen in den letzten 90 Tagen.",
            "Aucun incident au cours des 90 derniers jours.",
            "Brak incydentów w ostatnich 90 dniach.",
            "Ei häiriöitä viimeisen 90 päivän aikana.",
            "Inga incidenter de senaste 90 dagarna.",
            "Ningún incidente en los últimos 90 días.",
        ],
    ),
    (
        "status.maintenance",
        [
            "Scheduled maintenance",
            "Geplante Wartung",
            "Maintenance planifiée",
            "Zaplanowana konserwacja",
            "Ajastettu huolto",
            "Planerat underhåll",
            "Mantenimiento programado",
        ],
    ),
    (
        "status.none_maint",
        [
            "No scheduled maintenance.",
            "Keine geplante Wartung.",
            "Aucune maintenance planifiée.",
            "Brak zaplanowanej konserwacji.",
            "Ei ajastettua huoltoa.",
            "Inget planerat underhåll.",
            "Sin mantenimiento programado.",
        ],
    ),
    (
        "status.updated",
        [
            "Last checked {date}",
            "Zuletzt geprüft {date}",
            "Dernière vérification {date}",
            "Ostatnio sprawdzono {date}",
            "Viimeksi tarkistettu {date}",
            "Senast kontrollerad {date}",
            "Última comprobación {date}",
        ],
    ),
    (
        "status.never",
        [
            "Not checked yet",
            "Noch nicht geprüft",
            "Pas encore vérifié",
            "Jeszcze nie sprawdzono",
            "Ei vielä tarkistettu",
            "Inte kontrollerad än",
            "Aún sin comprobar",
        ],
    ),
    (
        "status.auto_on",
        [
            "Turn on auto refresh (60 s)",
            "Automatische Aktualisierung einschalten (60 s)",
            "Activer l’actualisation automatique (60 s)",
            "Włącz automatyczne odświeżanie (60 s)",
            "Ota automaattinen päivitys käyttöön (60 s)",
            "Slå på automatisk uppdatering (60 s)",
            "Activar la actualización automática (60 s)",
        ],
    ),
    (
        "status.auto_off",
        [
            "Turn off auto refresh",
            "Automatische Aktualisierung ausschalten",
            "Désactiver l’actualisation automatique",
            "Wyłącz automatyczne odświeżanie",
            "Poista automaattinen päivitys käytöstä",
            "Slå av automatisk uppdatering",
            "Desactivar la actualización automática",
        ],
    ),
    (
        "status.feed",
        [
            "Subscribe to updates (Atom)",
            "Updates abonnieren (Atom)",
            "S’abonner aux mises à jour (Atom)",
            "Subskrybuj aktualizacje (Atom)",
            "Tilaa päivitykset (Atom)",
            "Prenumerera på uppdateringar (Atom)",
            "Suscribirse a las actualizaciones (Atom)",
        ],
    ),
    (
        "status.api_link",
        [
            "JSON API",
            "JSON-API",
            "API JSON",
            "API JSON",
            "JSON-rajapinta",
            "JSON-API",
            "API JSON",
        ],
    ),
    (
        "status.impact",
        [
            "Impact",
            "Auswirkung",
            "Impact",
            "Wpływ",
            "Vaikutus",
            "Påverkan",
            "Impacto",
        ],
    ),
    (
        "status.details",
        [
            "Details",
            "Details",
            "Détails",
            "Szczegóły",
            "Lisätiedot",
            "Detaljer",
            "Detalles",
        ],
    ),
    (
        "status.back",
        [
            "Back to status",
            "Zurück zum Status",
            "Retour à l’état",
            "Wróć do statusu",
            "Takaisin tilaan",
            "Tillbaka till status",
            "Volver al estado",
        ],
    ),
    (
        "status.not_found",
        [
            "Incident not found",
            "Störung nicht gefunden",
            "Incident introuvable",
            "Nie znaleziono incydentu",
            "Häiriötä ei löytynyt",
            "Incidenten hittades inte",
            "Incidente no encontrado",
        ],
    ),
    (
        "impact.none",
        [
            "None",
            "Keine",
            "Aucun",
            "Brak",
            "Ei mitään",
            "Ingen",
            "Ninguno",
        ],
    ),
    (
        "impact.minor",
        [
            "Minor",
            "Gering",
            "Mineur",
            "Niewielki",
            "Vähäinen",
            "Liten",
            "Menor",
        ],
    ),
    (
        "impact.major",
        [
            "Major",
            "Erheblich",
            "Majeur",
            "Poważny",
            "Merkittävä",
            "Stor",
            "Mayor",
        ],
    ),
    (
        "impact.critical",
        [
            "Critical",
            "Kritisch",
            "Critique",
            "Krytyczny",
            "Kriittinen",
            "Kritisk",
            "Crítico",
        ],
    ),
    (
        "inc.investigating",
        [
            "Investigating",
            "In Untersuchung",
            "Investigation en cours",
            "Analizowanie",
            "Selvitetään",
            "Undersöks",
            "Investigando",
        ],
    ),
    (
        "inc.identified",
        [
            "Identified",
            "Ursache gefunden",
            "Cause identifiée",
            "Zidentyfikowano",
            "Tunnistettu",
            "Identifierad",
            "Identificado",
        ],
    ),
    (
        "inc.monitoring",
        [
            "Monitoring",
            "Wird beobachtet",
            "Surveillance",
            "Monitorowanie",
            "Seurannassa",
            "Övervakas",
            "En observación",
        ],
    ),
    (
        "inc.resolved",
        [
            "Resolved",
            "Behoben",
            "Résolu",
            "Rozwiązano",
            "Ratkaistu",
            "Åtgärdad",
            "Resuelto",
        ],
    ),
    (
        "inc.affected",
        [
            "Affected components: {names}",
            "Betroffene Komponenten: {names}",
            "Composants concernés : {names}",
            "Dotknięte komponenty: {names}",
            "Vaikutuksen kohteena: {names}",
            "Påverkade komponenter: {names}",
            "Componentes afectados: {names}",
        ],
    ),
    (
        "inc.started",
        [
            "Started {date}",
            "Beginn {date}",
            "Début {date}",
            "Początek {date}",
            "Alkoi {date}",
            "Började {date}",
            "Inicio {date}",
        ],
    ),
    (
        "inc.resolved_at",
        [
            "Resolved {date}",
            "Behoben am {date}",
            "Résolu le {date}",
            "Rozwiązano {date}",
            "Ratkaistu {date}",
            "Åtgärdades {date}",
            "Resuelto el {date}",
        ],
    ),
    (
        "inc.updates",
        [
            "Updates",
            "Aktualisierungen",
            "Mises à jour",
            "Aktualizacje",
            "Päivitykset",
            "Uppdateringar",
            "Actualizaciones",
        ],
    ),
    (
        "inc.degraded_list",
        [
            "Degraded instances: {names}",
            "Beeinträchtigte Instanzen: {names}",
            "Instances dégradées : {names}",
            "Instancje ze spadkiem wydajności: {names}",
            "Heikentyneet instanssit: {names}",
            "Försämrade instanser: {names}",
            "Instancias degradadas: {names}",
        ],
    ),
    (
        "inc.pct_up",
        [
            "{pct} of instances respond",
            "{pct} der Instanzen antworten",
            "{pct} des instances répondent",
            "{pct} instancji odpowiada",
            "{pct} instansseista vastaa",
            "{pct} av instanserna svarar",
            "{pct} de las instancias responden",
        ],
    ),
    (
        "inc.auto",
        [
            "Opened automatically",
            "Automatisch eröffnet",
            "Ouvert automatiquement",
            "Otwarty automatycznie",
            "Avattu automaattisesti",
            "Öppnad automatiskt",
            "Abierto automáticamente",
        ],
    ),
    (
        "maint.window",
        [
            "{from} to {to}",
            "{from} bis {to}",
            "du {from} au {to}",
            "{from} – {to}",
            "{from} – {to}",
            "{from} till {to}",
            "del {from} al {to}",
        ],
    ),
    (
        "maint.upcoming",
        [
            "Upcoming",
            "Geplant",
            "À venir",
            "Nadchodząca",
            "Tulossa",
            "Kommande",
            "Próximo",
        ],
    ),
    (
        "maint.in_progress",
        [
            "In progress",
            "Läuft",
            "En cours",
            "W toku",
            "Käynnissä",
            "Pågår",
            "En curso",
        ],
    ),
    (
        "adm.title",
        [
            "Status administration",
            "Statusverwaltung",
            "Administration de l’état",
            "Administracja statusu",
            "Tilan hallinta",
            "Statusadministration",
            "Administración del estado",
        ],
    ),
    (
        "adm.new_incident",
        [
            "Report an incident",
            "Störung melden",
            "Signaler un incident",
            "Zgłoś incydent",
            "Ilmoita häiriö",
            "Rapportera en incident",
            "Notificar un incidente",
        ],
    ),
    (
        "adm.add_update",
        [
            "Post an update",
            "Aktualisierung veröffentlichen",
            "Publier une mise à jour",
            "Opublikuj aktualizację",
            "Julkaise päivitys",
            "Publicera en uppdatering",
            "Publicar una actualización",
        ],
    ),
    (
        "adm.status_label",
        [
            "Status", "Status", "Statut", "Status", "Tila", "Status", "Estado",
        ],
    ),
    (
        "adm.components_label",
        [
            "Affected components",
            "Betroffene Komponenten",
            "Composants concernés",
            "Dotknięte komponenty",
            "Vaikutuksen kohteena olevat komponentit",
            "Påverkade komponenter",
            "Componentes afectados",
        ],
    ),
    (
        "adm.create",
        [
            "Create incident",
            "Störung anlegen",
            "Créer l’incident",
            "Utwórz incydent",
            "Luo häiriö",
            "Skapa incident",
            "Crear incidente",
        ],
    ),
    (
        "adm.schedule_title",
        [
            "Schedule maintenance",
            "Wartung planen",
            "Planifier une maintenance",
            "Zaplanuj konserwację",
            "Ajasta huolto",
            "Planera underhåll",
            "Programar mantenimiento",
        ],
    ),
    (
        "adm.starts",
        [
            "Starts (UTC, YYYY-MM-DD HH:MM)",
            "Beginn (UTC, JJJJ-MM-TT HH:MM)",
            "Début (UTC, AAAA-MM-JJ HH:MM)",
            "Początek (UTC, RRRR-MM-DD GG:MM)",
            "Alkaa (UTC, VVVV-KK-PP TT:MM)",
            "Börjar (UTC, ÅÅÅÅ-MM-DD TT:MM)",
            "Inicio (UTC, AAAA-MM-DD HH:MM)",
        ],
    ),
    (
        "adm.ends",
        [
            "Ends (UTC, YYYY-MM-DD HH:MM)",
            "Ende (UTC, JJJJ-MM-TT HH:MM)",
            "Fin (UTC, AAAA-MM-JJ HH:MM)",
            "Koniec (UTC, RRRR-MM-DD GG:MM)",
            "Päättyy (UTC, VVVV-KK-PP TT:MM)",
            "Slutar (UTC, ÅÅÅÅ-MM-DD TT:MM)",
            "Fin (UTC, AAAA-MM-DD HH:MM)",
        ],
    ),
    (
        "adm.schedule",
        [
            "Schedule",
            "Planen",
            "Planifier",
            "Zaplanuj",
            "Ajasta",
            "Planera",
            "Programar",
        ],
    ),
    (
        "adm.invalid",
        [
            "Check the fields: title, times and components are required.",
            "Prüfen Sie die Felder: Titel, Zeiten und Komponenten sind erforderlich.",
            "Vérifiez les champs : titre, horaires et composants sont obligatoires.",
            "Sprawdź pola: tytuł, czasy i komponenty są wymagane.",
            "Tarkista kentät: otsikko, ajat ja komponentit vaaditaan.",
            "Kontrollera fälten: rubrik, tider och komponenter krävs.",
            "Revisa los campos: el título, las horas y los componentes son obligatorios.",
        ],
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(s: &str) -> Vec<String> {
        let mut v: Vec<String> = s
            .split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}').map(|(n, _)| n.to_string()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn every_row_is_complete_and_placeholders_match() {
        let mut seen = std::collections::HashSet::new();
        for (key, cols) in CATALOGUE {
            assert!(seen.insert(*key), "duplicate key {key}");
            let want = placeholders(cols[0]);
            for (i, c) in cols.iter().enumerate() {
                assert!(!c.trim().is_empty(), "{key} column {i} is empty");
                assert_eq!(
                    placeholders(c),
                    want,
                    "{key} column {i} placeholders differ"
                );
                if i > 0
                    && ![
                        "nav.community",
                        "nav.status",
                        "comp.community",
                        "comp.provisioning",
                        "adm.status_label",
                        "mod.panel",
                        "mod.who",
                        "status.api_link",
                        "status.details",
                        "forum.pagination",
                        "forum.staff",
                    ]
                    .contains(key)
                    && c.len() > 40
                {
                    assert_ne!(c, &cols[0], "{key} column {i} is untranslated");
                }
            }
        }
    }

    #[test]
    fn lookup_and_formatting_per_language() {
        assert_eq!(t(Lang::De, "nav.login"), "Anmelden");
        assert_eq!(t(Lang::Pl, "nav.login"), "Zaloguj się");
        assert_eq!(t(Lang::En, "no.such.key"), "?");
        assert_eq!(tf(Lang::Fr, "forum.by", &[("name", "ada")]), "par ada");
        // 2026-10-16 12:30 UTC
        let ts = 1_791_936_000 + 2 * 86_400 + 12 * 3600 + 30 * 60;
        assert_eq!(util::iso_date(ts), "2026-10-16");
        assert_eq!(fmt_date(Lang::En, ts), "16 Oct 2026");
        assert_eq!(fmt_date(Lang::De, ts), "16. Okt. 2026");
        assert_eq!(fmt_date(Lang::Fi, ts), "16. lokakuuta 2026");
        assert_eq!(fmt_date(Lang::Es, ts), "16 oct 2026");
        assert_eq!(fmt_datetime(Lang::Sv, ts), "16 okt. 2026, 12:30 UTC");
        assert_eq!(fmt_num(Lang::En, 1_234_567), "1,234,567");
        assert_eq!(fmt_num(Lang::De, 1_234_567), "1.234.567");
        assert_eq!(fmt_num(Lang::Fr, 12_345), "12\u{202f}345");
        assert_eq!(fmt_num(Lang::Es, 1234), "1234");
        assert_eq!(fmt_num(Lang::En, 999), "999");
        assert_eq!(fmt_pct(Lang::En, 99.95), "99.95%");
        assert_eq!(fmt_pct(Lang::De, 100.0), "100\u{a0}%");
        assert_eq!(fmt_pct(Lang::Pl, 99.5), "99,5%");
    }

    #[test]
    fn language_negotiation_order() {
        assert_eq!(
            negotiate(Some("de"), Some("fr"), Some("pl")),
            (Lang::De, Source::Query)
        );
        assert_eq!(
            negotiate(Some("xx"), Some("fr"), Some("pl")),
            (Lang::Fr, Source::Cookie)
        );
        assert_eq!(
            negotiate(None, Some("zz"), Some("pl,en;q=0.5")),
            (Lang::Pl, Source::Header)
        );
        assert_eq!(negotiate(None, None, None), (Lang::En, Source::Default));
        assert_eq!(
            negotiate(None, None, Some("xx, yy;q=0.4")),
            (Lang::En, Source::Default)
        );
        assert_eq!(
            negotiate(None, None, Some("en;q=0.3, sv-SE;q=0.9, fi;q=0.8")).0,
            Lang::Sv
        );
        assert_eq!(negotiate(None, None, Some("de;q=0, es")).0, Lang::Es);
        assert_eq!(Lang::from_code("pt-BR"), None);
        assert_eq!(Lang::from_code("FI-fi"), Some(Lang::Fi));
    }

    /// `requested` is `negotiate` without the English default: the same order, and `None` when no
    /// source named one of the seven languages.
    #[test]
    fn a_requested_language_is_one_a_source_really_named() {
        assert_eq!(requested(None, None, None), None);
        assert_eq!(
            requested(Some("xx"), Some("zz"), Some("xx, yy;q=0.4")),
            None
        );
        assert_eq!(requested(Some(""), Some(""), Some("")), None);
        assert_eq!(
            requested(Some("de"), Some("fr"), Some("pl")),
            Some(Lang::De)
        );
        assert_eq!(
            requested(Some("xx"), Some("fr"), Some("pl")),
            Some(Lang::Fr)
        );
        assert_eq!(
            requested(None, Some("zz"), Some("pl,en;q=0.5")),
            Some(Lang::Pl)
        );
        // A region reads as its language; q-values order the header; q=0 means "not acceptable".
        assert_eq!(
            requested(None, None, Some("en;q=0.3, sv-SE;q=0.9, fi;q=0.8")),
            Some(Lang::Sv)
        );
        assert_eq!(requested(None, None, Some("de;q=0, es")), Some(Lang::Es));
        // English asked for is English asked for: it beats a cookie in another language.
        assert_eq!(requested(Some("en"), Some("sv"), None), Some(Lang::En));
    }
}
