//! The server-rendered pages of the authorization flow: sign in, second factor, consent, and the
//! error page. Plain HTML and one inline stylesheet, no script: the content security policy
//! allows only that stylesheet (by hash) and sends forms to this origin and, on the consent
//! page, to the origin the person will be sent back to. Text comes from a catalogue in the seven
//! languages of the site (`en` `de` `fr` `pl` `fi` `sv` `es`); the language is the `lang`
//! parameter, then the `Accept-Language` header, then English. Everything that came from outside
//! (the application's name and address, group names) is escaped.

use crate::flow::{DEFAULT_DAYS, EXPIRY_DAYS, FlowCtx};
use crate::util::b64std;
use sha2::{Digest, Sha256};

pub const LANGS: [&str; 7] = ["en", "de", "fr", "pl", "fi", "sv", "es"];

/// A finished page and the content security policy to send with it.
pub struct Rendered {
    pub html: String,
    pub csp: String,
}

/// `lang` parameter, then `Accept-Language` (q-values honoured), then `en`.
pub fn pick_lang(param: Option<&str>, accept: Option<&str>) -> &'static str {
    let known = |tag: &str| {
        let primary = tag
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        LANGS.into_iter().find(|l| *l == primary)
    };
    if let Some(l) = param.and_then(known) {
        return l;
    }
    let mut best: Option<(f32, usize, &'static str)> = None;
    for (i, part) in accept.unwrap_or("").split(',').enumerate() {
        let mut it = part.split(';');
        let tag = it.next().unwrap_or("").trim();
        let q = it
            .find_map(|p| {
                p.trim()
                    .strip_prefix("q=")
                    .and_then(|v| v.parse::<f32>().ok())
            })
            .unwrap_or(1.0);
        if q <= 0.0 {
            continue;
        }
        if let Some(l) = known(tag)
            && best.is_none_or(|(bq, bi, _)| q > bq || (q == bq && i < bi))
        {
            best = Some((q, i, l));
        }
    }
    best.map_or("en", |(_, _, l)| l)
}

fn lang_idx(lang: &str) -> usize {
    LANGS.iter().position(|l| *l == lang).unwrap_or(0)
}

/// The text for `key`; `{name}` placeholders are replaced by the (already plain) `args`.
pub fn t(lang: &str, key: &str) -> &'static str {
    match CATALOGUE.iter().find(|(k, _)| *k == key) {
        Some((_, cols)) if !cols[lang_idx(lang)].is_empty() => cols[lang_idx(lang)],
        Some((_, cols)) => cols[0],
        None => "",
    }
}

fn tf(lang: &str, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = esc(t(lang, key));
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), &format!("<strong>{}</strong>", esc(v)));
    }
    s
}

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

const CSS: &str = r#"
:root{--bg:#f6f7f9;--card:#fff;--fg:#14181f;--muted:#4b5565;--line:#d5dae2;--accent:#1d4ed8;--accent-fg:#fff;--danger:#b42318;--danger-bg:#fef3f2;--warn-bg:#fffaeb;--warn-line:#f2c86b;color-scheme:light dark}
@media (prefers-color-scheme:dark){:root{--bg:#0e1116;--card:#171b22;--fg:#eceff4;--muted:#a3adbd;--line:#2c3341;--accent:#6ea0ff;--accent-fg:#0b1020;--danger:#ff8f85;--danger-bg:#3a1713;--warn-bg:#2b2210;--warn-line:#7a5d17}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
main{max-width:34rem;margin:0 auto;padding:24px 16px 48px}
.brand{font-weight:700;letter-spacing:.02em;margin:0 0 16px}
.card{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:20px}
h1{font-size:1.35rem;line-height:1.3;margin:0 0 12px}
h2{font-size:1rem;margin:20px 0 6px}
p{margin:8px 0}
.muted{color:var(--muted)}
.notice{border:1px solid var(--warn-line);background:var(--warn-bg);border-radius:8px;padding:10px 12px;margin:12px 0}
.error{border:1px solid var(--danger);background:var(--danger-bg);color:var(--danger);border-radius:8px;padding:10px 12px;margin:12px 0}
label{display:block;font-weight:600;margin:12px 0 4px}
input[type=text],input[type=password],select{width:100%;font:inherit;color:var(--fg);background:var(--bg);border:1px solid var(--line);border-radius:8px;padding:10px 12px;min-height:44px}
fieldset{border:1px solid var(--line);border-radius:8px;margin:12px 0;padding:8px 12px 12px}
legend{font-weight:600;padding:0 6px}
.opt{display:flex;gap:10px;align-items:flex-start;font-weight:400;margin:8px 0;min-height:32px}
.opt input{margin-top:5px;width:20px;height:20px;flex:none}
.tag{font-size:.8rem;border:1px solid var(--warn-line);background:var(--warn-bg);border-radius:999px;padding:0 8px;margin-left:6px}
ul{padding-left:1.2rem;margin:8px 0}
li{margin:4px 0}
.row{display:flex;gap:12px;margin-top:20px;flex-wrap:wrap}
button{font:inherit;font-weight:600;border-radius:8px;border:1px solid var(--line);background:var(--card);color:var(--fg);padding:10px 18px;min-height:44px;cursor:pointer}
button.primary{background:var(--accent);border-color:var(--accent);color:var(--accent-fg)}
button.link{border:0;background:none;color:var(--accent);padding:4px 0;min-height:32px;text-decoration:underline;font-weight:400}
button:focus-visible,input:focus-visible,select:focus-visible{outline:3px solid var(--accent);outline-offset:2px}
.row button{flex:1 1 8rem}
code{overflow-wrap:anywhere}
"#;

fn csp(form_action: &str) -> String {
    let hash = b64std(&Sha256::digest(CSS.as_bytes()));
    format!(
        "default-src 'none'; style-src 'sha256-{hash}'; form-action 'self'{form_action}; frame-ancestors 'none'; base-uri 'none'"
    )
}

fn shell(lang: &str, title: &str, body: &str, form_action: &str) -> Rendered {
    let html = format!(
        "<!doctype html>\n<html lang=\"{lang}\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<meta name=\"color-scheme\" content=\"light dark\"><meta name=\"referrer\" content=\"same-origin\">\
<meta name=\"robots\" content=\"noindex,nofollow\"><title>{title}</title><style>{CSS}</style></head>\
<body><main><p class=\"brand\">obop</p><div class=\"card\">{body}</div></main></body></html>\n",
        title = esc(title)
    );
    Rendered {
        html,
        csp: csp(form_action),
    }
}

fn error_box(lang: &str, key: Option<&str>) -> String {
    match key {
        Some(k) => format!(
            "<p class=\"error\" role=\"alert\"><strong>{}:</strong> {}</p>",
            esc(t(lang, "err.label")),
            esc(t(lang, k))
        ),
        None => String::new(),
    }
}

fn hidden(ctx: &FlowCtx) -> String {
    format!(
        "<input type=\"hidden\" name=\"flow\" value=\"{}\"><input type=\"hidden\" name=\"csrf\" value=\"{}\">",
        esc(&ctx.id),
        esc(&ctx.csrf)
    )
}

/// A page that ends the flow: the request cannot be continued.
pub fn error_page(lang: &str, key: &str) -> Rendered {
    let body = format!(
        "<h1>{}</h1><p>{}</p>",
        esc(t(lang, "err.title")),
        esc(t(lang, key))
    );
    shell(lang, t(lang, "err.title"), &body, "")
}

pub fn login_page(ctx: &FlowCtx, error: Option<&str>) -> Rendered {
    let l = ctx.lang.as_str();
    let body = format!(
        "<h1>{h1}</h1><p>{lead}</p>{err}\
<form method=\"post\" action=\"/oauth/authorize/login\">{hidden}\
<label for=\"u\">{user}</label><input id=\"u\" name=\"username\" type=\"text\" autocomplete=\"username\" autocapitalize=\"none\" spellcheck=\"false\" required autofocus>\
<label for=\"p\">{pw}</label><input id=\"p\" name=\"password\" type=\"password\" autocomplete=\"current-password\" required>\
<div class=\"row\"><button class=\"primary\" type=\"submit\">{submit}</button></div></form>",
        h1 = esc(t(l, "login.title")),
        lead = tf(l, "login.lead", &[("app", &ctx.client_name)]),
        err = error_box(l, error),
        hidden = hidden(ctx),
        user = esc(t(l, "login.user")),
        pw = esc(t(l, "login.password")),
        submit = esc(t(l, "login.submit")),
    );
    shell(l, t(l, "login.title"), &body, "")
}

pub fn mfa_page(ctx: &FlowCtx, mfa_token: &str, error: Option<&str>) -> Rendered {
    let l = ctx.lang.as_str();
    let body = format!(
        "<h1>{h1}</h1><p>{lead}</p>{err}\
<form method=\"post\" action=\"/oauth/authorize/mfa\">{hidden}<input type=\"hidden\" name=\"mfa_token\" value=\"{tok}\">\
<label for=\"c\">{code}</label><input id=\"c\" name=\"code\" type=\"text\" inputmode=\"numeric\" autocomplete=\"one-time-code\" autocapitalize=\"none\" spellcheck=\"false\" required autofocus>\
<div class=\"row\"><button class=\"primary\" type=\"submit\">{submit}</button></div></form>",
        h1 = esc(t(l, "mfa.title")),
        lead = esc(t(l, "mfa.lead")),
        err = error_box(l, error),
        hidden = hidden(ctx),
        tok = esc(mfa_token),
        code = esc(t(l, "mfa.code")),
        submit = esc(t(l, "mfa.submit")),
    );
    shell(l, t(l, "mfa.title"), &body, "")
}

fn is_admin_like(g: &str) -> bool {
    g.starts_with("platform.") || g.ends_with(".admin")
}

pub fn consent_page(ctx: &FlowCtx, error: Option<&str>) -> Rendered {
    let l = ctx.lang.as_str();
    let user = ctx.user.as_deref().unwrap_or("");
    let mut groups = String::new();
    for g in &ctx.groups {
        let checked = if ctx.preselected.contains(g) {
            " checked"
        } else {
            ""
        };
        let tag = if is_admin_like(g) {
            format!(
                "<span class=\"tag\">{}</span>",
                esc(t(l, "consent.admin_tag"))
            )
        } else {
            String::new()
        };
        groups.push_str(&format!(
            "<label class=\"opt\"><input type=\"checkbox\" name=\"group\" value=\"{g}\"{checked}><span><code>{g}</code>{tag}</span></label>",
            g = esc(g)
        ));
    }
    if ctx.groups.is_empty() {
        groups.push_str(&format!(
            "<p class=\"muted\">{}</p>",
            esc(t(l, "consent.no_groups"))
        ));
    }
    let mut days = String::new();
    for d in EXPIRY_DAYS {
        let key = format!("consent.d{d}");
        let sel = if d == DEFAULT_DAYS { " selected" } else { "" };
        days.push_str(&format!(
            "<option value=\"{d}\"{sel}>{}</option>",
            esc(t(l, &key))
        ));
    }
    let body = format!(
        "<h1>{h1}</h1>\
<p class=\"muted\">{signed}</p>\
<form method=\"post\" action=\"/oauth/authorize/decision\">{hidden}<button class=\"link\" type=\"submit\" name=\"decision\" value=\"switch\" formnovalidate>{switch}</button></form>\
{err}\
<div class=\"notice\"><strong>{who}</strong><p>{who_body}</p></div>\
<form method=\"post\" action=\"/oauth/authorize/decision\">{hidden}\
<fieldset><legend>{glegend}</legend><p class=\"muted\">{ghelp}</p>{groups}</fieldset>\
<label for=\"days\">{expiry}</label><select id=\"days\" name=\"days\">{days}</select>\
<h2>{facts}</h2><ul><li>{f1}</li><li>{f2}</li><li>{f3}</li></ul>\
<div class=\"row\"><button type=\"submit\" name=\"decision\" value=\"deny\" formnovalidate>{deny}</button>\
<button class=\"primary\" type=\"submit\" name=\"decision\" value=\"allow\">{allow}</button></div></form>",
        h1 = tf(l, "consent.title", &[("app", &ctx.client_name)]),
        signed = tf(l, "consent.signed_in", &[("user", user)]),
        hidden = hidden(ctx),
        switch = esc(t(l, "consent.switch")),
        err = error_box(l, error),
        who = esc(t(l, "consent.who")),
        who_body = tf(
            l,
            "consent.who_body",
            &[("app", &ctx.client_name), ("host", &ctx.redirect_host)]
        ),
        glegend = esc(t(l, "consent.groups")),
        ghelp = esc(t(l, "consent.groups_help")),
        groups = groups,
        expiry = esc(t(l, "consent.expiry")),
        days = days,
        facts = esc(t(l, "consent.facts")),
        f1 = esc(t(l, "consent.f1")),
        f2 = esc(t(l, "consent.f2")),
        f3 = esc(t(l, "consent.f3")),
        deny = esc(t(l, "consent.deny")),
        allow = esc(t(l, "consent.allow")),
    );
    let action = if ctx.redirect_origin.is_empty() {
        String::new()
    } else {
        format!(" {}", ctx.redirect_origin)
    };
    shell(
        l,
        &tf_plain(l, "consent.title", &ctx.client_name),
        &body,
        &action,
    )
}

fn tf_plain(lang: &str, key: &str, app: &str) -> String {
    t(lang, key).replace("{app}", app)
}

/// Text of the pages. One row per key, columns in the order of [`LANGS`]:
/// en, de, fr, pl, fi, sv, es. `{name}` placeholders must appear in every column. The six
/// translations are machine-drafted and await review by native speakers (docs/ISSUES.md D7).
#[rustfmt::skip]
pub const CATALOGUE: &[(&str, [&str; 7])] = &[
    ("err.label", ["Error", "Fehler", "Erreur", "Błąd", "Virhe", "Fel", "Error"]),
    ("err.title", ["This cannot continue", "Das geht so nicht weiter", "Impossible de continuer", "Nie można kontynuować", "Tämä ei voi jatkua", "Det går inte att fortsätta", "No se puede continuar"]),
    ("err.client", ["This application is not registered with this instance, or its registration has expired. Start the connection again from the application.", "Diese Anwendung ist bei dieser Instanz nicht registriert, oder ihre Registrierung ist abgelaufen. Starten Sie die Verbindung erneut in der Anwendung.", "Cette application n'est pas enregistrée auprès de cette instance, ou son enregistrement a expiré. Relancez la connexion depuis l'application.", "Ta aplikacja nie jest zarejestrowana w tej instancji lub jej rejestracja wygasła. Rozpocznij połączenie ponownie w aplikacji.", "Tätä sovellusta ei ole rekisteröity tähän instanssiin tai sen rekisteröinti on vanhentunut. Aloita yhteys uudelleen sovelluksesta.", "Programmet är inte registrerat hos den här instansen, eller så har registreringen gått ut. Starta anslutningen igen från programmet.", "Esta aplicación no está registrada en esta instancia, o su registro ha caducado. Inicie la conexión de nuevo desde la aplicación."]),
    ("err.redirect", ["The return address of this request is not one the application registered, so it was not followed.", "Die Rücksprungadresse dieser Anfrage gehört nicht zu den registrierten Adressen der Anwendung und wurde daher nicht verwendet.", "L'adresse de retour de cette demande n'est pas celle que l'application a enregistrée ; elle n'a donc pas été suivie.", "Adres powrotu tego żądania nie jest adresem zarejestrowanym przez aplikację, więc nie został użyty.", "Pyynnön paluuosoite ei ole sovelluksen rekisteröimä, joten sitä ei käytetty.", "Återvändningsadressen i begäran är inte en som programmet har registrerat, så den följdes inte.", "La dirección de retorno de esta solicitud no es una de las que la aplicación registró, así que no se utilizó."]),
    ("err.gone", ["This sign-in expired or was already used. Start the connection again from the application.", "Diese Anmeldung ist abgelaufen oder wurde bereits verwendet. Starten Sie die Verbindung erneut in der Anwendung.", "Cette connexion a expiré ou a déjà été utilisée. Relancez la connexion depuis l'application.", "To logowanie wygasło lub zostało już użyte. Rozpocznij połączenie ponownie w aplikacji.", "Tämä kirjautuminen on vanhentunut tai jo käytetty. Aloita yhteys uudelleen sovelluksesta.", "Den här inloggningen har gått ut eller redan använts. Starta anslutningen igen från programmet.", "Este inicio de sesión ha caducado o ya se usó. Inicie la conexión de nuevo desde la aplicación."]),
    ("err.forbidden", ["This request did not come from the page that started the sign-in, so it was refused. Start again from the application.", "Diese Anfrage stammt nicht von der Seite, die die Anmeldung gestartet hat, und wurde abgelehnt. Beginnen Sie erneut in der Anwendung.", "Cette requête ne vient pas de la page qui a lancé la connexion ; elle a été refusée. Recommencez depuis l'application.", "To żądanie nie pochodzi ze strony, która rozpoczęła logowanie, więc zostało odrzucone. Zacznij od nowa w aplikacji.", "Pyyntö ei tullut kirjautumisen aloittaneelta sivulta, joten se hylättiin. Aloita alusta sovelluksesta.", "Begäran kom inte från sidan som startade inloggningen och nekades. Börja om från programmet.", "Esta solicitud no procede de la página que inició el acceso, así que se rechazó. Empiece de nuevo desde la aplicación."]),
    ("err.busy", ["Too many attempts. Wait a minute and try again.", "Zu viele Versuche. Warten Sie eine Minute und versuchen Sie es erneut.", "Trop de tentatives. Attendez une minute et réessayez.", "Zbyt wiele prób. Poczekaj minutę i spróbuj ponownie.", "Liian monta yritystä. Odota minuutti ja yritä uudelleen.", "För många försök. Vänta en minut och försök igen.", "Demasiados intentos. Espere un minuto e inténtelo de nuevo."]),
    ("err.login", ["The username or password is not right.", "Benutzername oder Passwort ist nicht richtig.", "Le nom d'utilisateur ou le mot de passe n'est pas correct.", "Nazwa użytkownika lub hasło jest nieprawidłowe.", "Käyttäjätunnus tai salasana ei ole oikein.", "Användarnamnet eller lösenordet stämmer inte.", "El usuario o la contraseña no son correctos."]),
    ("err.locked", ["Too many wrong attempts. Wait a minute and try again.", "Zu viele falsche Versuche. Warten Sie eine Minute und versuchen Sie es erneut.", "Trop de tentatives échouées. Attendez une minute et réessayez.", "Zbyt wiele błędnych prób. Poczekaj minutę i spróbuj ponownie.", "Liian monta virheellistä yritystä. Odota minuutti ja yritä uudelleen.", "För många felaktiga försök. Vänta en minut och försök igen.", "Demasiados intentos fallidos. Espere un minuto e inténtelo de nuevo."]),
    ("err.enrol", ["Your administrator requires two-factor authentication. Sign in to the back office and set it up first, then connect again.", "Ihr Administrator verlangt die Zwei-Faktor-Authentifizierung. Melden Sie sich im Backoffice an und richten Sie sie zuerst ein, verbinden Sie dann erneut.", "Votre administrateur exige l'authentification à deux facteurs. Connectez-vous au back-office pour la configurer, puis reconnectez-vous.", "Administrator wymaga uwierzytelniania dwuskładnikowego. Zaloguj się do panelu, skonfiguruj je, a potem połącz ponownie.", "Ylläpitäjäsi vaatii kaksivaiheisen tunnistautumisen. Kirjaudu taustajärjestelmään, ota se käyttöön ja yhdistä sitten uudelleen.", "Din administratör kräver tvåfaktorsautentisering. Logga in i administrationen, ställ in den först och anslut sedan igen.", "Su administrador exige la autenticación en dos pasos. Inicie sesión en el back office, configúrela primero y vuelva a conectar."]),
    ("err.pwchange", ["You must choose a new password in the back office before you can connect an application.", "Sie müssen im Backoffice ein neues Passwort wählen, bevor Sie eine Anwendung verbinden können.", "Vous devez choisir un nouveau mot de passe dans le back-office avant de pouvoir connecter une application.", "Zanim połączysz aplikację, musisz ustawić nowe hasło w panelu.", "Sinun on valittava uusi salasana taustajärjestelmässä ennen kuin voit yhdistää sovelluksen.", "Du måste välja ett nytt lösenord i administrationen innan du kan ansluta ett program.", "Debe elegir una contraseña nueva en el back office antes de poder conectar una aplicación."]),
    ("err.code", ["That code is not right.", "Dieser Code ist nicht richtig.", "Ce code n'est pas correct.", "Ten kod jest nieprawidłowy.", "Koodi ei ole oikein.", "Koden stämmer inte.", "Ese código no es correcto."]),
    ("err.code_expired", ["The sign-in took too long. Enter your username and password again.", "Die Anmeldung hat zu lange gedauert. Geben Sie Benutzername und Passwort erneut ein.", "La connexion a pris trop de temps. Saisissez à nouveau votre identifiant et votre mot de passe.", "Logowanie trwało zbyt długo. Wpisz nazwę użytkownika i hasło ponownie.", "Kirjautuminen kesti liian kauan. Syötä käyttäjätunnus ja salasana uudelleen.", "Inloggningen tog för lång tid. Ange användarnamn och lösenord igen.", "El inicio de sesión tardó demasiado. Introduzca de nuevo su usuario y contraseña."]),
    ("err.scope_widen", ["You can only hand over groups you hold yourself.", "Sie können nur Gruppen weitergeben, die Sie selbst haben.", "Vous ne pouvez transmettre que les groupes que vous détenez.", "Możesz przekazać tylko grupy, które sam posiadasz.", "Voit antaa vain ryhmiä, jotka itse omistat.", "Du kan bara lämna över grupper som du själv har.", "Solo puede ceder grupos que usted mismo tiene."]),
    ("err.no_scope", ["Choose at least one group, or deny.", "Wählen Sie mindestens eine Gruppe oder lehnen Sie ab.", "Choisissez au moins un groupe, ou refusez.", "Wybierz co najmniej jedną grupę albo odmów.", "Valitse vähintään yksi ryhmä tai kieltäydy.", "Välj minst en grupp, eller neka.", "Elija al menos un grupo, o deniegue."]),
    ("err.expiry", ["Choose how long the access lasts.", "Wählen Sie, wie lange der Zugriff gilt.", "Choisissez la durée de l'accès.", "Wybierz, jak długo ma trwać dostęp.", "Valitse, kuinka kauan pääsy kestää.", "Välj hur länge åtkomsten gäller.", "Elija cuánto dura el acceso."]),
    ("login.title", ["Sign in to continue", "Zum Fortfahren anmelden", "Connectez-vous pour continuer", "Zaloguj się, aby kontynuować", "Kirjaudu jatkaaksesi", "Logga in för att fortsätta", "Inicie sesión para continuar"]),
    ("login.lead", ["{app} wants to connect to this obop instance. Sign in with your obop account to decide what it may do.", "{app} möchte sich mit dieser obop-Instanz verbinden. Melden Sie sich mit Ihrem obop-Konto an, um zu entscheiden, was die Anwendung darf.", "{app} souhaite se connecter à cette instance obop. Connectez-vous avec votre compte obop pour décider de ce qu'elle peut faire.", "{app} chce połączyć się z tą instancją obop. Zaloguj się kontem obop, aby zdecydować, na co aplikacja może sobie pozwolić.", "{app} haluaa yhdistää tähän obop-instanssiin. Kirjaudu obop-tililläsi päättääksesi, mitä se saa tehdä.", "{app} vill ansluta till den här obop-instansen. Logga in med ditt obop-konto för att bestämma vad den får göra.", "{app} quiere conectarse a esta instancia de obop. Inicie sesión con su cuenta de obop para decidir qué puede hacer."]),
    ("login.user", ["Username", "Benutzername", "Nom d'utilisateur", "Nazwa użytkownika", "Käyttäjätunnus", "Användarnamn", "Usuario"]),
    ("login.password", ["Password", "Passwort", "Mot de passe", "Hasło", "Salasana", "Lösenord", "Contraseña"]),
    ("login.submit", ["Sign in", "Anmelden", "Se connecter", "Zaloguj się", "Kirjaudu", "Logga in", "Iniciar sesión"]),
    ("mfa.title", ["Two-factor code", "Zwei-Faktor-Code", "Code à deux facteurs", "Kod dwuskładnikowy", "Kaksivaiheinen koodi", "Tvåfaktorskod", "Código de dos pasos"]),
    ("mfa.lead", ["Enter the 6-digit code from your authenticator app, or one of your recovery codes.", "Geben Sie den 6-stelligen Code aus Ihrer Authenticator-App oder einen Ihrer Wiederherstellungscodes ein.", "Saisissez le code à 6 chiffres de votre application d'authentification, ou l'un de vos codes de récupération.", "Wpisz 6-cyfrowy kod z aplikacji uwierzytelniającej lub jeden z kodów odzyskiwania.", "Syötä todennussovelluksen 6-numeroinen koodi tai jokin palautuskoodeistasi.", "Ange den sexsiffriga koden från din autentiseringsapp eller en av dina återställningskoder.", "Introduzca el código de 6 dígitos de su aplicación de autenticación, o uno de sus códigos de recuperación."]),
    ("mfa.code", ["Code", "Code", "Code", "Kod", "Koodi", "Kod", "Código"]),
    ("mfa.submit", ["Verify", "Bestätigen", "Vérifier", "Zweryfikuj", "Vahvista", "Verifiera", "Verificar"]),
    ("consent.title", ["Connect {app}?", "{app} verbinden?", "Connecter {app} ?", "Połączyć {app}?", "Yhdistetäänkö {app}?", "Ansluta {app}?", "¿Conectar {app}?"]),
    ("consent.signed_in", ["Signed in as {user}.", "Angemeldet als {user}.", "Connecté en tant que {user}.", "Zalogowano jako {user}.", "Kirjautunut käyttäjänä {user}.", "Inloggad som {user}.", "Sesión iniciada como {user}."]),
    ("consent.switch", ["Not you? Use another account", "Nicht Sie? Anderes Konto verwenden", "Ce n'est pas vous ? Utiliser un autre compte", "To nie Ty? Użyj innego konta", "Etkö se ole sinä? Käytä toista tiliä", "Inte du? Använd ett annat konto", "¿No es usted? Usar otra cuenta"]),
    ("consent.who", ["Who is asking", "Wer fragt an", "Qui fait la demande", "Kto o to prosi", "Kuka pyytää", "Vem frågar", "Quién lo solicita"]),
    ("consent.who_body", ["{app} registered itself with this instance. obop cannot verify who runs it. After you answer, your browser goes to {host}. Only continue if you started this connection yourself.", "{app} hat sich selbst bei dieser Instanz registriert. obop kann nicht prüfen, wer sie betreibt. Nach Ihrer Antwort wechselt Ihr Browser zu {host}. Fahren Sie nur fort, wenn Sie diese Verbindung selbst gestartet haben.", "{app} s'est enregistrée elle-même auprès de cette instance. obop ne peut pas vérifier qui l'exploite. Après votre réponse, votre navigateur ira vers {host}. Ne continuez que si vous avez lancé cette connexion vous-même.", "{app} sama zarejestrowała się w tej instancji. obop nie może sprawdzić, kto ją prowadzi. Po Twojej odpowiedzi przeglądarka przejdzie do {host}. Kontynuuj tylko wtedy, gdy sam rozpocząłeś to połączenie.", "{app} on rekisteröitynyt itse tähän instanssiin. obop ei voi tarkistaa, kuka sitä ylläpitää. Vastauksesi jälkeen selaimesi siirtyy osoitteeseen {host}. Jatka vain, jos aloitit tämän yhteyden itse.", "{app} har registrerat sig själv hos den här instansen. obop kan inte kontrollera vem som driver det. Efter ditt svar går webbläsaren till {host}. Fortsätt bara om du själv startade anslutningen.", "{app} se registró por sí misma en esta instancia. obop no puede comprobar quién la gestiona. Tras su respuesta, el navegador irá a {host}. Continúe solo si usted mismo inició esta conexión."]),
    ("consent.groups", ["What it may do: permission groups to hand over", "Was erlaubt ist: Berechtigungsgruppen, die weitergegeben werden", "Ce qu'elle peut faire : groupes de permissions à transmettre", "Co może robić: grupy uprawnień do przekazania", "Mitä se saa tehdä: luovutettavat käyttöoikeusryhmät", "Vad den får göra: behörighetsgrupper att lämna över", "Qué puede hacer: grupos de permisos que se ceden"]),
    ("consent.groups_help", ["The application acts as you, with only the groups you tick. It can never do more than you can.", "Die Anwendung handelt als Sie, nur mit den angekreuzten Gruppen. Sie kann nie mehr als Sie selbst.", "L'application agit en votre nom, avec seulement les groupes cochés. Elle ne peut jamais faire plus que vous.", "Aplikacja działa w Twoim imieniu, tylko z zaznaczonymi grupami. Nigdy nie zrobi więcej niż Ty.", "Sovellus toimii puolestasi vain valitsemillasi ryhmillä. Se ei voi koskaan tehdä enempää kuin sinä.", "Programmet agerar som du, bara med de grupper du kryssar i. Det kan aldrig göra mer än du.", "La aplicación actúa como usted, solo con los grupos que marque. Nunca puede hacer más que usted."]),
    ("consent.admin_tag", ["administrative", "administrativ", "administratif", "administracyjna", "hallinnollinen", "administrativ", "administrativo"]),
    ("consent.no_groups", ["Your account holds no permission groups, so there is nothing to hand over.", "Ihr Konto hat keine Berechtigungsgruppen, es gibt also nichts weiterzugeben.", "Votre compte n'a aucun groupe de permissions : il n'y a rien à transmettre.", "Twoje konto nie ma grup uprawnień, więc nie ma czego przekazać.", "Tililläsi ei ole käyttöoikeusryhmiä, joten luovutettavaa ei ole.", "Ditt konto har inga behörighetsgrupper, så det finns inget att lämna över.", "Su cuenta no tiene grupos de permisos, así que no hay nada que ceder."]),
    ("consent.expiry", ["Access ends after", "Der Zugriff endet nach", "L'accès prend fin après", "Dostęp wygasa po", "Pääsy päättyy", "Åtkomsten upphör efter", "El acceso termina tras"]),
    ("consent.d1", ["1 day", "1 Tag", "1 jour", "1 dniu", "1 päivä", "1 dag", "1 día"]),
    ("consent.d7", ["7 days", "7 Tagen", "7 jours", "7 dniach", "7 päivää", "7 dagar", "7 días"]),
    ("consent.d30", ["30 days", "30 Tagen", "30 jours", "30 dniach", "30 päivää", "30 dagar", "30 días"]),
    ("consent.d90", ["90 days", "90 Tagen", "90 jours", "90 dniach", "90 päivää", "90 dagar", "90 días"]),
    ("consent.facts", ["Good to know", "Gut zu wissen", "Bon à savoir", "Warto wiedzieć", "Hyvä tietää", "Bra att veta", "Conviene saber"]),
    ("consent.f1", ["What the application does is recorded as an action of your agent, under your name.", "Was die Anwendung tut, wird als Aktion Ihres Agenten unter Ihrem Namen aufgezeichnet.", "Ce que fait l'application est enregistré comme une action de votre agent, sous votre nom.", "To, co robi aplikacja, jest zapisywane jako działanie Twojego agenta pod Twoim nazwiskiem.", "Sovelluksen toimet tallennetaan agenttisi toimina sinun nimissäsi.", "Det programmet gör registreras som en åtgärd av din agent, under ditt namn.", "Lo que hace la aplicación se registra como una acción de su agente, a su nombre."]),
    ("consent.f2", ["Steps that need approval still wait for a person to approve them in obop. The application cannot approve its own proposals.", "Schritte, die eine Freigabe brauchen, warten weiterhin darauf, dass eine Person sie in obop freigibt. Die Anwendung kann ihre eigenen Vorschläge nicht freigeben.", "Les étapes qui demandent une approbation attendent toujours qu'une personne les approuve dans obop. L'application ne peut pas approuver ses propres propositions.", "Kroki wymagające zatwierdzenia nadal czekają, aż człowiek zatwierdzi je w obop. Aplikacja nie może zatwierdzać własnych propozycji.", "Hyväksyntää vaativat vaiheet odottavat edelleen, että ihminen hyväksyy ne obopissa. Sovellus ei voi hyväksyä omia ehdotuksiaan.", "Steg som kräver godkännande väntar fortfarande på att en person godkänner dem i obop. Programmet kan inte godkänna sina egna förslag.", "Los pasos que requieren aprobación siguen esperando a que una persona los apruebe en obop. La aplicación no puede aprobar sus propias propuestas."]),
    ("consent.f3", ["You can end this at any time under Customize, Security, Connected apps.", "Sie können dies jederzeit unter Anpassen, Sicherheit, Verbundene Apps beenden.", "Vous pouvez y mettre fin à tout moment sous Personnaliser, Sécurité, Applications connectées.", "Możesz to zakończyć w dowolnej chwili w Dostosuj, Bezpieczeństwo, Połączone aplikacje.", "Voit lopettaa tämän milloin tahansa kohdassa Mukauta, Turvallisuus, Yhdistetyt sovellukset.", "Du kan avsluta detta när som helst under Anpassa, Säkerhet, Anslutna appar.", "Puede terminarlo en cualquier momento en Personalizar, Seguridad, Aplicaciones conectadas."]),
    ("consent.allow", ["Allow", "Erlauben", "Autoriser", "Zezwól", "Salli", "Tillåt", "Permitir"]),
    ("consent.deny", ["Deny", "Ablehnen", "Refuser", "Odmów", "Kiellä", "Neka", "Denegar"]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_exists_in_every_language_with_the_same_placeholders() {
        for (key, cols) in CATALOGUE {
            for (i, c) in cols.iter().enumerate() {
                assert!(!c.is_empty(), "{key} is empty in {}", LANGS[i]);
                for ph in ["{app}", "{user}", "{host}"] {
                    assert_eq!(
                        cols[0].contains(ph),
                        c.contains(ph),
                        "{key}: placeholder {ph} in {}",
                        LANGS[i]
                    );
                }
            }
        }
    }

    #[test]
    fn language_choice() {
        assert_eq!(pick_lang(Some("de"), Some("fr")), "de");
        assert_eq!(pick_lang(Some("xx"), Some("fi-FI,en;q=0.5")), "fi");
        assert_eq!(pick_lang(None, Some("pl;q=0.3, sv;q=0.8")), "sv");
        assert_eq!(pick_lang(None, None), "en");
        assert_eq!(pick_lang(None, Some("ja")), "en");
    }

    #[test]
    fn untrusted_text_is_escaped() {
        let ctx = FlowCtx {
            id: "f".into(),
            csrf: "c".into(),
            lang: "en".into(),
            client_name: "<script>alert(1)</script>\"'".into(),
            redirect_origin: "https://a.test".into(),
            redirect_host: "a.test".into(),
            user: Some("ada".into()),
            groups: vec!["base.user".into(), "x\"><b>".into()],
            preselected: vec![],
        };
        let r = consent_page(&ctx, None);
        assert!(!r.html.contains("<script>") && !r.html.contains("<b>"));
        assert!(r.csp.contains("form-action 'self' https://a.test"));
        assert!(r.csp.contains("default-src 'none'") && !r.csp.contains("unsafe"));
    }
}
