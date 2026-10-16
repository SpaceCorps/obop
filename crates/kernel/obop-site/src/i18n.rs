//! Languages of the public site's chrome: navigation, buttons, forms, errors, dates and money.
//! Records carry their own translations (see the `*_translation` models); this table is only the
//! text the site itself writes.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Query,
    Cookie,
    Header,
    Default,
}

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
        match self {
            Lang::En => "en",
            Lang::De => "de",
            Lang::Fr => "fr",
            Lang::Pl => "pl",
            Lang::Fi => "fi",
            Lang::Sv => "sv",
            Lang::Es => "es",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
            Lang::Fr => "Français",
            Lang::Pl => "Polski",
            Lang::Fi => "Suomi",
            Lang::Sv => "Svenska",
            Lang::Es => "Español",
        }
    }

    pub fn og_locale(self) -> &'static str {
        match self {
            Lang::En => "en_US",
            Lang::De => "de_DE",
            Lang::Fr => "fr_FR",
            Lang::Pl => "pl_PL",
            Lang::Fi => "fi_FI",
            Lang::Sv => "sv_SE",
            Lang::Es => "es_ES",
        }
    }

    /// BCP 47 tag used for number formatting decisions.
    pub fn parse(s: &str) -> Option<Lang> {
        let primary = s
            .trim()
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }

    fn idx(self) -> usize {
        Lang::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    /// Languages in `Accept-Language` order of preference (q-values honoured); the first one the
    /// site speaks.
    pub fn from_accept(h: &str) -> Option<Lang> {
        let mut best: Option<(f32, usize, Lang)> = None;
        for (i, part) in h.split(',').enumerate() {
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
            if let Some(l) = Lang::parse(tag)
                && best.is_none_or(|(bq, bi, _)| q > bq || (q == bq && i < bi))
            {
                best = Some((q, i, l));
            }
        }
        best.map(|(_, _, l)| l)
    }
}

/// Text of the chrome. One row per key, columns in the order of `Lang::ALL`:
/// en, de, fr, pl, fi, sv, es. `{name}` placeholders must appear in every column.
#[rustfmt::skip]
pub const CATALOGUE: &[(&str, [&str; 7])] = &[
    ("skip", ["Skip to content", "Zum Inhalt springen", "Aller au contenu", "Przejdź do treści", "Siirry sisältöön", "Hoppa till innehållet", "Saltar al contenido"]),
    ("nav.main", ["Main navigation", "Hauptnavigation", "Navigation principale", "Nawigacja główna", "Päävalikko", "Huvudmeny", "Navegación principal"]),
    ("nav.home", ["Home", "Startseite", "Accueil", "Strona główna", "Etusivu", "Hem", "Inicio"]),
    ("nav.shop", ["Shop", "Shop", "Boutique", "Sklep", "Kauppa", "Butik", "Tienda"]),
    ("nav.blog", ["Blog", "Blog", "Blog", "Blog", "Blogi", "Blogg", "Blog"]),
    ("nav.contact", ["Contact", "Kontakt", "Contact", "Kontakt", "Yhteystiedot", "Kontakt", "Contacto"]),
    ("nav.cart", ["Cart", "Warenkorb", "Panier", "Koszyk", "Ostoskori", "Varukorg", "Carrito"]),
    ("nav.cart_items", ["Cart, {n} items", "Warenkorb, {n} Artikel", "Panier, {n} articles", "Koszyk, liczba pozycji: {n}", "Ostoskori, {n} tuotetta", "Varukorg, {n} varor", "Carrito, {n} artículos"]),
    ("nav.language", ["Language", "Sprache", "Langue", "Język", "Kieli", "Språk", "Idioma"]),
    ("nav.breadcrumb", ["Breadcrumb", "Brotkrumen-Navigation", "Fil d'Ariane", "Ścieżka nawigacji", "Murupolku", "Sökväg", "Ruta de navegación"]),
    ("nav.social", ["Follow us", "Folgen Sie uns", "Suivez-nous", "Obserwuj nas", "Seuraa meitä", "Följ oss", "Síguenos"]),
    ("footer.md", ["Read this page as Markdown", "Diese Seite als Markdown lesen", "Lire cette page en Markdown", "Ta strona jako Markdown", "Lue tämä sivu Markdownina", "Läs sidan som Markdown", "Leer esta página en Markdown"]),
    ("footer.feed", ["Blog feed", "Blog-Feed", "Flux du blog", "Kanał bloga", "Blogin syöte", "Bloggflöde", "Canal del blog"]),
    ("footer.prices", ["Prices include tax.", "Preise inklusive Steuern.", "Prix TTC.", "Ceny zawierają podatek.", "Hinnat sisältävät veron.", "Priserna inkluderar moms.", "Los precios incluyen impuestos."]),
    ("home.browse", ["Browse the shop", "Zum Shop", "Voir la boutique", "Przeglądaj sklep", "Selaa kauppaa", "Utforska butiken", "Ver la tienda"]),
    ("home.featured", ["Featured products", "Ausgewählte Produkte", "Produits à la une", "Polecane produkty", "Esittelyssä", "Utvalda produkter", "Productos destacados"]),
    ("home.latest", ["From the blog", "Aus dem Blog", "Dans le blog", "Z bloga", "Blogista", "Från bloggen", "Del blog"]),
    ("home.all_products", ["All products", "Alle Produkte", "Tous les produits", "Wszystkie produkty", "Kaikki tuotteet", "Alla produkter", "Todos los productos"]),
    ("home.all_posts", ["All posts", "Alle Beiträge", "Tous les articles", "Wszystkie wpisy", "Kaikki kirjoitukset", "Alla inlägg", "Todas las entradas"]),
    ("unpub.title", ["This site is not published yet", "Diese Website ist noch nicht veröffentlicht", "Ce site n'est pas encore publié", "Ta witryna nie jest jeszcze opublikowana", "Tätä sivustoa ei ole vielä julkaistu", "Den här webbplatsen är inte publicerad än", "Este sitio todavía no está publicado"]),
    ("unpub.body", ["The owner has not published any pages yet. Please come back soon.", "Der Inhaber hat noch keine Seiten veröffentlicht. Bitte schauen Sie bald wieder vorbei.", "Le propriétaire n'a pas encore publié de pages. Revenez bientôt.", "Właściciel nie opublikował jeszcze żadnych stron. Zajrzyj tu wkrótce.", "Omistaja ei ole vielä julkaissut sivuja. Palaa pian uudelleen.", "Ägaren har inte publicerat några sidor än. Titta in igen snart.", "El propietario todavía no ha publicado ninguna página. Vuelva pronto."]),
    ("unpub.owner", ["Are you the owner? Sign in to the admin", "Sind Sie der Inhaber? Zum Admin-Bereich", "Vous êtes le propriétaire ? Accédez à l'administration", "Jesteś właścicielem? Przejdź do panelu administracyjnego", "Oletko omistaja? Kirjaudu hallintaan", "Är du ägaren? Logga in i administrationen", "¿Es usted el propietario? Acceda a la administración"]),
    ("closed.title", ["Temporarily closed", "Vorübergehend geschlossen", "Temporairement fermé", "Chwilowo zamknięte", "Tilapäisesti suljettu", "Tillfälligt stängt", "Cerrado temporalmente"]),
    ("closed.body", ["This site is taking a short break. Please try again later.", "Diese Website macht gerade eine kurze Pause. Bitte versuchen Sie es später erneut.", "Ce site fait une courte pause. Veuillez réessayer plus tard.", "Ta witryna ma krótką przerwę. Spróbuj ponownie później.", "Sivusto on pienellä tauolla. Yritä myöhemmin uudelleen.", "Webbplatsen tar en kort paus. Försök igen senare.", "Este sitio está en pausa. Inténtelo de nuevo más tarde."]),
    ("shop.title", ["Shop", "Shop", "Boutique", "Sklep", "Kauppa", "Butik", "Tienda"]),
    ("shop.intro", ["Everything we sell, in one place.", "Alles, was wir verkaufen, an einem Ort.", "Tout ce que nous vendons, au même endroit.", "Wszystko, co sprzedajemy, w jednym miejscu.", "Kaikki myymämme tuotteet yhdessä paikassa.", "Allt vi säljer, på ett ställe.", "Todo lo que vendemos, en un solo lugar."]),
    ("shop.categories", ["Categories", "Kategorien", "Catégories", "Kategorie", "Kategoriat", "Kategorier", "Categorías"]),
    ("shop.all", ["All", "Alle", "Tout", "Wszystkie", "Kaikki", "Alla", "Todos"]),
    ("shop.search", ["Search products", "Produkte suchen", "Rechercher des produits", "Szukaj produktów", "Hae tuotteita", "Sök produkter", "Buscar productos"]),
    ("shop.search_ph", ["Search by name", "Nach Name suchen", "Rechercher par nom", "Szukaj po nazwie", "Hae nimellä", "Sök på namn", "Buscar por nombre"]),
    ("shop.search_btn", ["Search", "Suchen", "Rechercher", "Szukaj", "Hae", "Sök", "Buscar"]),
    ("shop.count", ["{n} products", "{n} Produkte", "{n} produits", "Produkty: {n}", "{n} tuotetta", "{n} produkter", "{n} productos"]),
    ("shop.none", ["No products match your search.", "Keine Produkte entsprechen Ihrer Suche.", "Aucun produit ne correspond à votre recherche.", "Żaden produkt nie pasuje do wyszukiwania.", "Haulla ei löytynyt tuotteita.", "Inga produkter matchar din sökning.", "Ningún producto coincide con su búsqueda."]),
    ("shop.empty", ["There are no products yet.", "Es gibt noch keine Produkte.", "Il n'y a pas encore de produits.", "Nie ma jeszcze produktów.", "Tuotteita ei ole vielä.", "Det finns inga produkter än.", "Todavía no hay productos."]),
    ("shop.page", ["Page {n} of {m}", "Seite {n} von {m}", "Page {n} sur {m}", "Strona {n} z {m}", "Sivu {n} / {m}", "Sida {n} av {m}", "Página {n} de {m}"]),
    ("shop.prev", ["Previous", "Zurück", "Précédent", "Poprzednia", "Edellinen", "Föregående", "Anterior"]),
    ("shop.next", ["Next", "Weiter", "Suivant", "Następna", "Seuraava", "Nästa", "Siguiente"]),
    ("shop.pagination", ["Pages", "Seiten", "Pages", "Strony", "Sivut", "Sidor", "Páginas"]),
    ("shop.add", ["Add to cart", "In den Warenkorb", "Ajouter au panier", "Dodaj do koszyka", "Lisää ostoskoriin", "Lägg i varukorgen", "Añadir al carrito"]),
    ("shop.added", ["Added to your cart", "Zum Warenkorb hinzugefügt", "Ajouté à votre panier", "Dodano do koszyka", "Lisätty ostoskoriin", "Tillagd i varukorgen", "Añadido al carrito"]),
    ("shop.in_stock", ["In stock", "Auf Lager", "En stock", "Dostępny", "Varastossa", "I lager", "En stock"]),
    ("shop.out_of_stock", ["Out of stock", "Nicht auf Lager", "Rupture de stock", "Niedostępny", "Loppu varastosta", "Slut i lager", "Agotado"]),
    ("shop.backorder", ["Available on backorder", "Auf Nachbestellung lieferbar", "Disponible sur commande", "Dostępny na zamówienie", "Saatavilla ennakkotilauksella", "Kan förbeställas", "Disponible bajo pedido"]),
    ("shop.was", ["was {price}", "statt {price}", "au lieu de {price}", "poprzednio {price}", "aiemmin {price}", "tidigare {price}", "antes {price}"]),
    ("shop.sale", ["Sale", "Angebot", "Promo", "Promocja", "Tarjous", "Rea", "Oferta"]),
    ("shop.sku", ["SKU {sku}", "Art.-Nr. {sku}", "Réf. {sku}", "SKU {sku}", "Tuotenumero {sku}", "Art.nr {sku}", "Ref. {sku}"]),
    ("shop.qty", ["Quantity", "Menge", "Quantité", "Ilość", "Määrä", "Antal", "Cantidad"]),
    ("shop.product_image", ["Product", "Produkt", "Produit", "Produkt", "Tuote", "Produkt", "Producto"]),
    ("shop.back", ["Back to the shop", "Zurück zum Shop", "Retour à la boutique", "Wróć do sklepu", "Takaisin kauppaan", "Tillbaka till butiken", "Volver a la tienda"]),
    ("cart.title", ["Your cart", "Ihr Warenkorb", "Votre panier", "Twój koszyk", "Ostoskorisi", "Din varukorg", "Su carrito"]),
    ("cart.empty", ["Your cart is empty.", "Ihr Warenkorb ist leer.", "Votre panier est vide.", "Twój koszyk jest pusty.", "Ostoskorisi on tyhjä.", "Din varukorg är tom.", "Su carrito está vacío."]),
    ("cart.continue", ["Continue shopping", "Weiter einkaufen", "Continuer vos achats", "Kontynuuj zakupy", "Jatka ostoksia", "Fortsätt handla", "Seguir comprando"]),
    ("cart.remove", ["Remove", "Entfernen", "Retirer", "Usuń", "Poista", "Ta bort", "Quitar"]),
    ("cart.remove_item", ["Remove {name}", "{name} entfernen", "Retirer {name}", "Usuń: {name}", "Poista {name}", "Ta bort {name}", "Quitar {name}"]),
    ("cart.item", ["Item", "Artikel", "Article", "Pozycja", "Tuote", "Vara", "Artículo"]),
    ("cart.price", ["Price", "Preis", "Prix", "Cena", "Hinta", "Pris", "Precio"]),
    ("cart.line_total", ["Total", "Summe", "Total", "Razem", "Yhteensä", "Summa", "Total"]),
    ("cart.subtotal", ["Subtotal", "Zwischensumme", "Sous-total", "Suma częściowa", "Välisumma", "Delsumma", "Subtotal"]),
    ("cart.shipping", ["Delivery", "Lieferung", "Livraison", "Dostawa", "Toimitus", "Leverans", "Envío"]),
    ("cart.total", ["Total", "Gesamtsumme", "Total", "Do zapłaty", "Yhteensä", "Totalt", "Total"]),
    ("cart.free", ["Free", "Kostenlos", "Gratuite", "Bezpłatna", "Ilmainen", "Gratis", "Gratis"]),
    ("cart.days", ["{n} days", "{n} Tage", "{n} jours", "{n} dni", "{n} päivää", "{n} dagar", "{n} días"]),
    ("cart.js_note", ["Your cart is kept in this browser.", "Ihr Warenkorb wird in diesem Browser gespeichert.", "Votre panier est conservé dans ce navigateur.", "Koszyk jest zapisany w tej przeglądarce.", "Ostoskorisi säilyy tässä selaimessa.", "Din varukorg sparas i den här webbläsaren.", "Su carrito se guarda en este navegador."]),
    ("cart.nojs", ["Without JavaScript, enter product and quantity here.", "Ohne JavaScript geben Sie Produkt und Menge hier ein.", "Sans JavaScript, saisissez ici le produit et la quantité.", "Bez JavaScriptu wpisz produkt i ilość tutaj.", "Ilman JavaScriptiä syötä tuote ja määrä tähän.", "Utan JavaScript anger du produkt och antal här.", "Sin JavaScript, introduzca aquí el producto y la cantidad."]),
    ("cart.update", ["Update cart", "Warenkorb aktualisieren", "Mettre à jour le panier", "Zaktualizuj koszyk", "Päivitä ostoskori", "Uppdatera varukorgen", "Actualizar carrito"]),
    ("cart.no_payment", ["There is no online payment yet. After you place the order we will contact you with payment details.", "Online bezahlen ist noch nicht möglich. Nach Ihrer Bestellung melden wir uns mit den Zahlungsdetails.", "Le paiement en ligne n'est pas encore disponible. Après votre commande, nous vous contacterons avec les informations de paiement.", "Płatności online nie są jeszcze dostępne. Po złożeniu zamówienia skontaktujemy się z Tobą w sprawie płatności.", "Verkkomaksu ei ole vielä käytössä. Tilauksen jälkeen otamme yhteyttä ja kerromme maksutiedot.", "Det finns ingen onlinebetalning än. När du har lagt beställningen kontaktar vi dig med betalningsuppgifter.", "Todavía no hay pago en línea. Tras realizar el pedido nos pondremos en contacto con usted con los datos de pago."]),
    ("co.title", ["Your details", "Ihre Angaben", "Vos coordonnées", "Twoje dane", "Yhteystietosi", "Dina uppgifter", "Sus datos"]),
    ("co.name", ["Full name", "Vollständiger Name", "Nom complet", "Imię i nazwisko", "Koko nimi", "Fullständigt namn", "Nombre completo"]),
    ("co.email", ["Email address", "E-Mail-Adresse", "Adresse e-mail", "Adres e-mail", "Sähköpostiosoite", "E-postadress", "Correo electrónico"]),
    ("co.email_help", ["We send nothing else than your order details to it.", "Wir nutzen sie nur für Ihre Bestelldaten.", "Nous l'utilisons uniquement pour votre commande.", "Używamy go tylko w sprawie zamówienia.", "Käytämme sitä vain tilauksesi asioihin.", "Vi använder den bara för din beställning.", "Solo la usamos para su pedido."]),
    ("co.address", ["Delivery address", "Lieferadresse", "Adresse de livraison", "Adres dostawy", "Toimitusosoite", "Leveransadress", "Dirección de entrega"]),
    ("co.shipping", ["Delivery method", "Liefermethode", "Mode de livraison", "Sposób dostawy", "Toimitustapa", "Leveranssätt", "Método de envío"]),
    ("co.honeypot", ["Leave this field empty", "Dieses Feld leer lassen", "Laissez ce champ vide", "Zostaw to pole puste", "Jätä tämä kenttä tyhjäksi", "Lämna det här fältet tomt", "Deje este campo vacío"]),
    ("co.submit", ["Place order", "Bestellung aufgeben", "Passer la commande", "Złóż zamówienie", "Tee tilaus", "Lägg beställning", "Realizar pedido"]),
    ("co.required", ["required", "Pflichtfeld", "obligatoire", "wymagane", "pakollinen", "obligatoriskt", "obligatorio"]),
    ("co.errors", ["Please fix the following", "Bitte korrigieren Sie Folgendes", "Veuillez corriger ce qui suit", "Popraw następujące błędy", "Korjaa seuraavat kohdat", "Rätta till följande", "Corrija lo siguiente"]),
    ("err.email", ["Enter a valid email address.", "Geben Sie eine gültige E-Mail-Adresse ein.", "Saisissez une adresse e-mail valide.", "Podaj prawidłowy adres e-mail.", "Anna kelvollinen sähköpostiosoite.", "Ange en giltig e-postadress.", "Introduzca un correo electrónico válido."]),
    ("err.name", ["Enter your name.", "Geben Sie Ihren Namen ein.", "Saisissez votre nom.", "Podaj swoje imię i nazwisko.", "Anna nimesi.", "Ange ditt namn.", "Introduzca su nombre."]),
    ("err.address", ["Enter a delivery address.", "Geben Sie eine Lieferadresse ein.", "Saisissez une adresse de livraison.", "Podaj adres dostawy.", "Anna toimitusosoite.", "Ange en leveransadress.", "Introduzca una dirección de entrega."]),
    ("err.message", ["Write a message.", "Schreiben Sie eine Nachricht.", "Écrivez un message.", "Napisz wiadomość.", "Kirjoita viesti.", "Skriv ett meddelande.", "Escriba un mensaje."]),
    ("err.consent", ["Please agree to be contacted.", "Bitte stimmen Sie der Kontaktaufnahme zu.", "Veuillez accepter d'être contacté.", "Wyraź zgodę na kontakt.", "Hyväksy yhteydenotto.", "Godkänn att vi kontaktar dig.", "Acepte que le contactemos."]),
    ("err.empty_cart", ["Your cart is empty.", "Ihr Warenkorb ist leer.", "Votre panier est vide.", "Twój koszyk jest pusty.", "Ostoskorisi on tyhjä.", "Din varukorg är tom.", "Su carrito está vacío."]),
    ("err.quantity", ["Quantities must be whole numbers from 1 to 99.", "Mengen müssen ganze Zahlen von 1 bis 99 sein.", "Les quantités doivent être des nombres entiers de 1 à 99.", "Ilości muszą być liczbami całkowitymi od 1 do 99.", "Määrien on oltava kokonaislukuja väliltä 1–99.", "Antal måste vara heltal mellan 1 och 99.", "Las cantidades deben ser números enteros de 1 a 99."]),
    ("err.unavailable", ["An item in your cart is no longer available: {name}.", "Ein Artikel in Ihrem Warenkorb ist nicht mehr verfügbar: {name}.", "Un article de votre panier n'est plus disponible : {name}.", "Pozycja w koszyku nie jest już dostępna: {name}.", "Ostoskorisi tuote ei ole enää saatavilla: {name}.", "En vara i varukorgen är inte längre tillgänglig: {name}.", "Un artículo de su carrito ya no está disponible: {name}."]),
    ("err.stock", ["Not enough stock for {name}.", "Nicht genug Bestand für {name}.", "Stock insuffisant pour {name}.", "Za mało sztuk: {name}.", "Varasto ei riitä tuotteelle {name}.", "Inte tillräckligt i lager av {name}.", "No hay existencias suficientes de {name}."]),
    ("err.currency", ["All items in one order must use the same currency.", "Alle Artikel einer Bestellung müssen dieselbe Währung haben.", "Tous les articles d'une commande doivent avoir la même devise.", "Wszystkie pozycje zamówienia muszą mieć tę samą walutę.", "Kaikkien tilauksen tuotteiden on oltava samassa valuutassa.", "Alla varor i en beställning måste ha samma valuta.", "Todos los artículos de un pedido deben usar la misma moneda."]),
    ("err.too_many", ["Your cart has too many different items.", "Ihr Warenkorb enthält zu viele verschiedene Artikel.", "Votre panier contient trop d'articles différents.", "W koszyku jest zbyt wiele różnych pozycji.", "Ostoskorissasi on liian monta eri tuotetta.", "Din varukorg har för många olika varor.", "Su carrito tiene demasiados artículos distintos."]),
    ("err.shipping", ["Choose a delivery method.", "Wählen Sie eine Liefermethode.", "Choisissez un mode de livraison.", "Wybierz sposób dostawy.", "Valitse toimitustapa.", "Välj ett leveranssätt.", "Elija un método de envío."]),
    ("err.csrf", ["Your session expired. Please reload the page and try again.", "Ihre Sitzung ist abgelaufen. Bitte laden Sie die Seite neu und versuchen Sie es erneut.", "Votre session a expiré. Rechargez la page et réessayez.", "Sesja wygasła. Odśwież stronę i spróbuj ponownie.", "Istunto vanheni. Lataa sivu uudelleen ja yritä uudestaan.", "Din session har gått ut. Ladda om sidan och försök igen.", "Su sesión ha caducado. Recargue la página e inténtelo de nuevo."]),
    ("err.rate", ["Too many requests from your network. Please try again later.", "Zu viele Anfragen aus Ihrem Netzwerk. Bitte versuchen Sie es später erneut.", "Trop de requêtes depuis votre réseau. Veuillez réessayer plus tard.", "Zbyt wiele żądań z Twojej sieci. Spróbuj ponownie później.", "Verkostasi tuli liian monta pyyntöä. Yritä myöhemmin uudelleen.", "För många förfrågningar från ditt nätverk. Försök igen senare.", "Demasiadas solicitudes desde su red. Inténtelo de nuevo más tarde."]),
    ("err.generic", ["Something went wrong. Please try again.", "Etwas ist schiefgelaufen. Bitte versuchen Sie es erneut.", "Une erreur s'est produite. Veuillez réessayer.", "Coś poszło nie tak. Spróbuj ponownie.", "Jotain meni pieleen. Yritä uudelleen.", "Något gick fel. Försök igen.", "Algo salió mal. Inténtelo de nuevo."]),
    ("err.too_large", ["That request was too large.", "Diese Anfrage war zu groß.", "Cette requête était trop volumineuse.", "To żądanie było zbyt duże.", "Pyyntö oli liian suuri.", "Förfrågan var för stor.", "La solicitud era demasiado grande."]),
    ("order.title", ["Thank you for your order", "Vielen Dank für Ihre Bestellung", "Merci pour votre commande", "Dziękujemy za zamówienie", "Kiitos tilauksestasi", "Tack för din beställning", "Gracias por su pedido"]),
    ("order.number", ["Order {number}", "Bestellung {number}", "Commande {number}", "Zamówienie {number}", "Tilaus {number}", "Beställning {number}", "Pedido {number}"]),
    ("order.next", ["We have received your order. There is no online payment yet, so we will contact you at {email} with payment details.", "Wir haben Ihre Bestellung erhalten. Online bezahlen ist noch nicht möglich, deshalb melden wir uns unter {email} mit den Zahlungsdetails.", "Nous avons bien reçu votre commande. Le paiement en ligne n'est pas encore disponible : nous vous écrirons à {email} avec les informations de paiement.", "Otrzymaliśmy Twoje zamówienie. Płatności online nie są jeszcze dostępne, więc napiszemy na {email} w sprawie płatności.", "Olemme vastaanottaneet tilauksesi. Verkkomaksu ei ole vielä käytössä, joten lähetämme maksutiedot osoitteeseen {email}.", "Vi har tagit emot din beställning. Det finns ingen onlinebetalning än, så vi kontaktar dig på {email} med betalningsuppgifter.", "Hemos recibido su pedido. Todavía no hay pago en línea, así que le escribiremos a {email} con los datos de pago."]),
    ("order.keep", ["Keep the link to this page. It is the only way to see your order again.", "Speichern Sie den Link zu dieser Seite. Nur damit können Sie Ihre Bestellung wieder ansehen.", "Conservez le lien de cette page. C'est le seul moyen de revoir votre commande.", "Zachowaj link do tej strony. To jedyny sposób, by ponownie zobaczyć zamówienie.", "Tallenna tämän sivun osoite. Sen avulla näet tilauksesi uudelleen.", "Spara länken till den här sidan. Det är det enda sättet att se din beställning igen.", "Guarde el enlace de esta página. Es la única forma de volver a ver su pedido."]),
    ("order.status", ["Status", "Status", "Statut", "Status", "Tila", "Status", "Estado"]),
    ("order.ship_to", ["Delivery to", "Lieferung an", "Livraison à", "Dostawa do", "Toimitus osoitteeseen", "Leverans till", "Entrega a"]),
    ("order.items", ["Items", "Artikel", "Articles", "Pozycje", "Tuotteet", "Varor", "Artículos"]),
    ("order.placed", ["Placed on {date}", "Aufgegeben am {date}", "Passée le {date}", "Złożone dnia {date}", "Tehty {date}", "Lagd {date}", "Realizado el {date}"]),
    ("state.pending", ["Pending, waiting for payment", "Offen, wartet auf Zahlung", "En attente de paiement", "Oczekuje na płatność", "Odottaa maksua", "Väntar på betalning", "Pendiente de pago"]),
    ("state.paid", ["Paid", "Bezahlt", "Payée", "Opłacone", "Maksettu", "Betald", "Pagado"]),
    ("state.shipped", ["Shipped", "Versendet", "Expédiée", "Wysłane", "Lähetetty", "Skickad", "Enviado"]),
    ("state.delivered", ["Delivered", "Geliefert", "Livrée", "Dostarczone", "Toimitettu", "Levererad", "Entregado"]),
    ("state.cancelled", ["Cancelled", "Storniert", "Annulée", "Anulowane", "Peruttu", "Avbruten", "Cancelado"]),
    ("state.refunded", ["Refunded", "Erstattet", "Remboursée", "Zwrócone", "Hyvitetty", "Återbetald", "Reembolsado"]),
    ("contact.title", ["Contact us", "Kontakt aufnehmen", "Contactez-nous", "Skontaktuj się z nami", "Ota yhteyttä", "Kontakta oss", "Contáctenos"]),
    ("contact.intro", ["Write to us and we will get back to you.", "Schreiben Sie uns, wir melden uns bei Ihnen.", "Écrivez-nous, nous vous répondrons.", "Napisz do nas, odpowiemy.", "Kirjoita meille, niin vastaamme.", "Skriv till oss så återkommer vi.", "Escríbanos y le responderemos."]),
    ("contact.name", ["Your name", "Ihr Name", "Votre nom", "Twoje imię i nazwisko", "Nimesi", "Ditt namn", "Su nombre"]),
    ("contact.email", ["Your email address", "Ihre E-Mail-Adresse", "Votre adresse e-mail", "Twój adres e-mail", "Sähköpostiosoitteesi", "Din e-postadress", "Su correo electrónico"]),
    ("contact.phone", ["Phone (optional)", "Telefon (optional)", "Téléphone (facultatif)", "Telefon (opcjonalnie)", "Puhelin (valinnainen)", "Telefon (valfritt)", "Teléfono (opcional)"]),
    ("contact.message", ["Your message", "Ihre Nachricht", "Votre message", "Twoja wiadomość", "Viestisi", "Ditt meddelande", "Su mensaje"]),
    ("contact.consent", ["I agree that you contact me about my message.", "Ich bin damit einverstanden, dass Sie mich wegen meiner Nachricht kontaktieren.", "J'accepte que vous me contactiez au sujet de mon message.", "Zgadzam się, aby skontaktowano się ze mną w sprawie mojej wiadomości.", "Hyväksyn, että minuun otetaan yhteyttä viestini vuoksi.", "Jag godkänner att ni kontaktar mig angående mitt meddelande.", "Acepto que se pongan en contacto conmigo sobre mi mensaje."]),
    ("contact.send", ["Send message", "Nachricht senden", "Envoyer le message", "Wyślij wiadomość", "Lähetä viesti", "Skicka meddelande", "Enviar mensaje"]),
    ("contact.thanks_title", ["Message sent", "Nachricht gesendet", "Message envoyé", "Wiadomość wysłana", "Viesti lähetetty", "Meddelandet skickades", "Mensaje enviado"]),
    ("contact.thanks", ["Thank you. We will get back to you soon.", "Vielen Dank. Wir melden uns in Kürze bei Ihnen.", "Merci. Nous vous répondrons bientôt.", "Dziękujemy. Wkrótce się odezwiemy.", "Kiitos. Vastaamme pian.", "Tack. Vi återkommer snart.", "Gracias. Le responderemos pronto."]),
    ("blog.title", ["Blog", "Blog", "Blog", "Blog", "Blogi", "Blogg", "Blog"]),
    ("blog.intro", ["News and notes from us.", "Neuigkeiten und Notizen von uns.", "Nouvelles et notes de notre part.", "Nowości i notatki od nas.", "Uutisia ja muistiinpanoja meiltä.", "Nyheter och anteckningar från oss.", "Novedades y notas de nuestra parte."]),
    ("blog.empty", ["There are no posts yet.", "Es gibt noch keine Beiträge.", "Il n'y a pas encore d'articles.", "Nie ma jeszcze wpisów.", "Kirjoituksia ei ole vielä.", "Det finns inga inlägg än.", "Todavía no hay entradas."]),
    ("blog.by", ["By {name}", "Von {name}", "Par {name}", "Autor: {name}", "Kirjoittanut {name}", "Av {name}", "Por {name}"]),
    ("blog.back", ["All posts", "Alle Beiträge", "Tous les articles", "Wszystkie wpisy", "Kaikki kirjoitukset", "Alla inlägg", "Todas las entradas"]),
    ("blog.read", ["Read more", "Weiterlesen", "Lire la suite", "Czytaj dalej", "Lue lisää", "Läs mer", "Leer más"]),
    ("e404.title", ["Page not found", "Seite nicht gefunden", "Page introuvable", "Nie znaleziono strony", "Sivua ei löytynyt", "Sidan hittades inte", "Página no encontrada"]),
    ("e404.body", ["We could not find that page. It may have moved or never existed.", "Diese Seite wurde nicht gefunden. Sie wurde vielleicht verschoben oder hat nie existiert.", "Cette page est introuvable. Elle a peut-être été déplacée ou n'a jamais existé.", "Nie znaleziono tej strony. Mogła zostać przeniesiona lub nigdy nie istniała.", "Sivua ei löytynyt. Se on voinut siirtyä tai sitä ei ole koskaan ollut.", "Vi hittar inte sidan. Den kan ha flyttats eller aldrig ha funnits.", "No encontramos esa página. Puede que se haya movido o que nunca haya existido."]),
    ("e410.title", ["This page is gone", "Diese Seite gibt es nicht mehr", "Cette page n'existe plus", "Ta strona została usunięta", "Tämä sivu on poistettu", "Den här sidan finns inte kvar", "Esta página ya no existe"]),
    ("e410.body", ["It was retired and will not come back.", "Sie wurde endgültig entfernt und kommt nicht wieder.", "Elle a été retirée définitivement.", "Została wycofana na stałe.", "Se on poistettu pysyvästi.", "Den har tagits bort för gott.", "Se retiró de forma definitiva."]),
    ("e405.title", ["Method not allowed", "Methode nicht erlaubt", "Méthode non autorisée", "Metoda niedozwolona", "Menetelmä ei ole sallittu", "Metoden tillåts inte", "Método no permitido"]),
    ("e429.title", ["Too many requests", "Zu viele Anfragen", "Trop de requêtes", "Zbyt wiele żądań", "Liian monta pyyntöä", "För många förfrågningar", "Demasiadas solicitudes"]),
    ("e413.title", ["Request too large", "Anfrage zu groß", "Requête trop volumineuse", "Żądanie zbyt duże", "Pyyntö liian suuri", "Förfrågan för stor", "Solicitud demasiado grande"]),
    ("e403.title", ["Request refused", "Anfrage abgelehnt", "Requête refusée", "Żądanie odrzucone", "Pyyntö hylätty", "Förfrågan nekades", "Solicitud rechazada"]),
];

/// The text for `key` in `lang`; falls back to English, and is empty for an unknown key (a test
/// proves that every key the pages use exists).
pub fn t(lang: Lang, key: &str) -> &'static str {
    match CATALOGUE.iter().find(|(k, _)| *k == key) {
        Some((_, cols)) if !cols[lang.idx()].is_empty() => cols[lang.idx()],
        Some((_, cols)) => cols[0],
        None => "",
    }
}

/// `t` with `{name}` placeholders replaced. Values are inserted as they are: the caller escapes
/// when the result goes into HTML.
pub fn tf(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(lang, key).to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

const MONTHS: [[&str; 12]; 7] = [
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ],
    [
        "stycznia",
        "lutego",
        "marca",
        "kwietnia",
        "maja",
        "czerwca",
        "lipca",
        "sierpnia",
        "września",
        "października",
        "listopada",
        "grudnia",
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
        "januari",
        "februari",
        "mars",
        "april",
        "maj",
        "juni",
        "juli",
        "augusti",
        "september",
        "oktober",
        "november",
        "december",
    ],
    [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
];

/// A calendar date `YYYY-MM-DD` written the way the language writes it: `October 16, 2026`,
/// `16. Oktober 2026`, `16 octobre 2026`, `16 października 2026`, `16. lokakuuta 2026`,
/// `16 oktober 2026`, `16 de octubre de 2026`. Anything that is not a date is returned as is.
pub fn fmt_date(lang: Lang, iso: &str) -> String {
    let p: Vec<&str> = iso.split('-').collect();
    let (Some(y), Some(m), Some(d)) = (
        p.first().and_then(|x| x.parse::<i32>().ok()),
        p.get(1).and_then(|x| x.parse::<usize>().ok()),
        p.get(2).and_then(|x| x.parse::<u32>().ok()),
    ) else {
        return iso.to_string();
    };
    if p.len() != 3 || !(1..=12).contains(&m) || d == 0 || d > 31 {
        return iso.to_string();
    }
    let month = MONTHS[lang.idx()][m - 1];
    match lang {
        Lang::En => format!("{month} {d}, {y}"),
        Lang::De | Lang::Fi => format!("{d}. {month} {y}"),
        Lang::Es => format!("{d} de {month} de {y}"),
        Lang::Fr | Lang::Pl | Lang::Sv => format!("{d} {month} {y}"),
    }
}

fn group_sep(lang: Lang) -> &'static str {
    match lang {
        Lang::En => ",",
        Lang::De | Lang::Es => ".",
        // narrow no-break space (fr) and no-break space (pl, fi, sv)
        Lang::Fr => "\u{202f}",
        Lang::Pl | Lang::Fi | Lang::Sv => "\u{a0}",
    }
}

fn decimal_sep(lang: Lang) -> &'static str {
    if lang == Lang::En { "." } else { "," }
}

/// Digits after the decimal point for a currency (ISO 4217 minor unit).
pub fn minor_digits(currency: &str) -> u32 {
    match currency {
        "JPY" | "KRW" | "VND" | "CLP" | "ISK" | "UGX" | "XAF" | "XOF" | "PYG" | "RWF" => 0,
        "BHD" | "KWD" | "OMR" | "JOD" | "TND" | "IQD" | "LYD" => 3,
        _ => 2,
    }
}

fn symbol(currency: &str) -> Option<&'static str> {
    Some(match currency {
        "EUR" => "€",
        "USD" => "$",
        "GBP" => "£",
        "JPY" => "¥",
        "PLN" => "zł",
        "SEK" | "NOK" | "DKK" | "ISK" => "kr",
        "CZK" => "Kč",
        "INR" => "₹",
        "KRW" => "₩",
        _ => return None,
    })
}

/// Group the digits of a non-negative integer. Spanish does not group four-digit numbers.
fn group_digits(lang: Lang, digits: &str) -> String {
    if lang == Lang::Es && digits.len() <= 4 {
        return digits.to_string();
    }
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(group_sep(lang));
        }
        out.push(c);
    }
    out
}

/// An integer with the language's thousands separator.
pub fn fmt_num(lang: Lang, n: u64) -> String {
    group_digits(lang, &n.to_string())
}

/// An amount in minor units of `currency` as the visitor's language writes it: `€1,234.50`,
/// `1.234,50 €`, `1 234,50 €`, with the symbol before the number in English and after it (with a
/// no-break space) elsewhere. Currencies without a known symbol are shown by code.
pub fn fmt_money(lang: Lang, minor: i64, currency: &str) -> String {
    let digits = minor_digits(currency);
    let neg = minor < 0;
    let abs = minor.unsigned_abs();
    let unit = 10u64.pow(digits);
    let whole = group_digits(lang, &(abs / unit).to_string());
    let num = if digits == 0 {
        whole
    } else {
        format!(
            "{whole}{}{:0width$}",
            decimal_sep(lang),
            abs % unit,
            width = digits as usize
        )
    };
    let sign = if neg { "\u{2212}" } else { "" };
    let sym = symbol(currency).unwrap_or(currency);
    if lang == Lang::En {
        let gap = if symbol(currency).is_some() {
            ""
        } else {
            "\u{a0}"
        };
        format!("{sign}{sym}{gap}{num}")
    } else {
        format!("{sign}{num}\u{a0}{sym}")
    }
}

/// Today as `YYYY-MM-DD` (UTC).
pub fn today() -> String {
    util::iso_date(util::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            let Some(j) = rest[i..].find('}') else { break };
            out.insert(rest[i + 1..i + j].to_string());
            rest = &rest[i + j + 1..];
        }
        out
    }

    #[test]
    fn every_key_is_complete_and_placeholders_match_in_every_language() {
        let mut seen = BTreeSet::new();
        for (key, cols) in CATALOGUE {
            assert!(seen.insert(*key), "duplicate key {key}");
            let want = placeholders(cols[0]);
            for (i, c) in cols.iter().enumerate() {
                assert!(!c.trim().is_empty(), "{key} column {i} is empty");
                assert_eq!(
                    placeholders(c),
                    want,
                    "{key} column {i}: placeholders differ"
                );
            }
        }
    }

    #[test]
    fn only_the_shared_words_may_match_english() {
        // A translation that is identical to English is allowed only for short shared words;
        // this catches a column that was left untranslated.
        for (key, cols) in CATALOGUE {
            for (i, c) in cols.iter().enumerate().skip(1) {
                if c == &cols[0] {
                    assert!(
                        c.chars().count() <= 22 && !c.contains(' ') || c.chars().count() <= 14,
                        "{key} column {i} looks untranslated: {c}"
                    );
                }
            }
        }
    }

    #[test]
    fn lookup_falls_back_and_unknown_keys_are_empty() {
        assert_eq!(t(Lang::De, "nav.home"), "Startseite");
        assert_eq!(t(Lang::En, "nav.home"), "Home");
        assert_eq!(t(Lang::De, "no.such.key"), "");
        assert_eq!(
            tf(Lang::Fr, "shop.page", &[("n", "2"), ("m", "5")]),
            "Page 2 sur 5"
        );
    }

    #[test]
    fn language_choice_parses_headers_and_tags() {
        assert_eq!(Lang::parse("de-AT"), Some(Lang::De));
        assert_eq!(Lang::parse("EN_gb"), Some(Lang::En));
        assert_eq!(Lang::parse("ja"), None);
        assert_eq!(
            Lang::from_accept("fr-CH, fr;q=0.9, en;q=0.8"),
            Some(Lang::Fr)
        );
        assert_eq!(Lang::from_accept("ja, sv;q=0.5, de;q=0.9"), Some(Lang::De));
        assert_eq!(Lang::from_accept("ja, zh"), None);
        assert_eq!(Lang::from_accept("de;q=0, en"), Some(Lang::En));
    }

    #[test]
    fn money_is_written_the_way_each_language_writes_it() {
        assert_eq!(fmt_money(Lang::En, 123_450, "EUR"), "€1,234.50");
        assert_eq!(fmt_money(Lang::De, 123_450, "EUR"), "1.234,50\u{a0}€");
        assert_eq!(
            fmt_money(Lang::Fr, 123_450, "EUR"),
            "1\u{202f}234,50\u{a0}€"
        );
        assert_eq!(fmt_money(Lang::Pl, 123_450, "PLN"), "1\u{a0}234,50\u{a0}zł");
        assert_eq!(fmt_money(Lang::Fi, 5, "EUR"), "0,05\u{a0}€");
        assert_eq!(fmt_money(Lang::Sv, 99_900, "SEK"), "999,00\u{a0}kr");
        assert_eq!(fmt_money(Lang::Es, 123_450, "EUR"), "1234,50\u{a0}€");
        assert_eq!(fmt_money(Lang::Es, 12_345_000, "EUR"), "123.450,00\u{a0}€");
        assert_eq!(fmt_money(Lang::En, 1999, "CHF"), "CHF\u{a0}19.99");
        assert_eq!(fmt_money(Lang::En, 1500, "JPY"), "¥1,500");
        assert_eq!(fmt_money(Lang::En, 12_345, "KWD"), "KWD\u{a0}12.345");
        assert_eq!(fmt_money(Lang::En, -250, "USD"), "\u{2212}$2.50");
    }

    #[test]
    fn dates_are_written_the_way_each_language_writes_them() {
        let d = "2026-10-16";
        assert_eq!(fmt_date(Lang::En, d), "October 16, 2026");
        assert_eq!(fmt_date(Lang::De, d), "16. Oktober 2026");
        assert_eq!(fmt_date(Lang::Fr, d), "16 octobre 2026");
        assert_eq!(fmt_date(Lang::Pl, d), "16 października 2026");
        assert_eq!(fmt_date(Lang::Fi, d), "16. lokakuuta 2026");
        assert_eq!(fmt_date(Lang::Sv, d), "16 oktober 2026");
        assert_eq!(fmt_date(Lang::Es, "2026-03-01"), "1 de marzo de 2026");
        assert_eq!(fmt_date(Lang::En, "not a date"), "not a date");
        assert_eq!(fmt_date(Lang::En, "2026-13-01"), "2026-13-01");
    }
}
