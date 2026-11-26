---
title: "E-faktury dla Niemiec i Szwecji: co budujemy"
description: "Niemcy wprowadzają od 2028 roku obowiązek ustrukturyzowanych e-faktur dla każdej firmy, a szwedzcy zamawiający publiczni już je przyjmują. Tak obop zamierza je czytać, sprawdzać i tworzyć, a to istnieje już dziś."
date: "2026-11-25"
---

Od 2028-01-01 każda firma z siedzibą w Niemczech musi wysyłać faktury do innych firm w Niemczech jako ustrukturyzowane e-faktury. PDF już nie wystarczy. To główny powód, dla którego Niemcy są pierwszym krajem obop, a e-faktury to praca, którą właśnie zaczęliśmy. Szwecja wymaga tej samej normy europejskiej dla faktur do zamawiających publicznych. Ten wpis mówi, czego wymaga prawo, co obop ma dziś i co budujemy.

## Czego wymaga prawo

Prawo niemieckie (§ 14 UStG) definiuje e-fakturę jako fakturę wystawioną, przesłaną i odebraną w ustrukturyzowanym formacie elektronicznym. Ten format musi być zgodny z europejską normą fakturowania elektronicznego EN 16931 albo być innym formatem uzgodnionym przez obie strony, który nadal pozwala na elektroniczne przetwarzanie. Faktury trzeba przechowywać przez osiem lat, z zapewnioną autentycznością pochodzenia, integralnością treści i czytelnością (§ 14b UStG). obop przechowuje plik XML odebranej e-faktury bez zmian. Obowiązek wchodzi etapami:

- **Od 2025-01-01:** każda firma musi być w stanie odbierać e-faktury.
- **Od 2027-01-01:** firmy z obrotem powyżej 800 000 EUR w poprzednim roku muszą je wystawiać.
- **Od 2028-01-01:** każda firma musi je wystawiać.

Wyjątki obejmują m.in. faktury do 250 EUR (§ 33 UStDV) oraz, do końca 2027 roku, procedury EDI uzgodnione z odbiorcą (§ 27 Abs. 38 UStG).

Niemcy akceptują kilka formatów. obop planuje obsłużyć trzy z nich: XRechnung, ZUGFeRD 2.5.2 (to samo co Factur-X 1.09.2, PDF z XML w środku) i Peppol BIS Billing 3.0. Za tymi formatami stoją dwie składnie XML, UBL i CII, i obie niosą tę samą treść EN 16931.

## Szwecja

W Szwecji obowiązek dotyczy faktur do sektora publicznego. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) weszła w życie 2019-04-01. W zamówieniach wszczętych od tego dnia faktury dostawcy do zamawiającego publicznego muszą być fakturami elektronicznymi zgodnymi z europejską normą EN 16931 (4 §), a zamawiający publiczni muszą takie faktury odbierać i przetwarzać (5 §). Dostawca i zamawiający mogą uzgodnić inną normę. Ustawa nie wskazuje formatu. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), odpowiedzialny od 2026-07-01 za powiązane przepisy, zaleca Peppol BIS Billing 3, implementację EN 16931, a [urzędy państwowe muszą go obsługiwać](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Zamawiający publiczni muszą być zarejestrowani w Peppol jako odbiorcy faktur i muszą móc odbierać zarówno UBL, jak i CII.

## Co obop ma dziś

obop ma aplikację Fakturowanie. Przechowuje faktury dla klientów i ich pozycje, sumuje kwotę i nadaje każdej zaksięgowanej fakturze kolejny numer z ciągłej rocznej serii. Agent AI może tylko zaproponować zaksięgowanie faktury; księguje ją człowiek.

Fakturowanie nie obsługuje e-faktur. Nie potrafi utworzyć pliku XRechnung, Factur-X ani Peppol BIS, nie potrafi go odczytać i niczego nie sprawdza względem EN 16931. Faktura nie ma też jeszcze zestawienia VAT według stawek, a norma go wymaga.

## Co budujemy

Praca ma dwie połowy. Zaczynamy od czytania, bo od 2025 roku firmy muszą być gotowe na odbiór e-faktur:

1. **Czytanie i sprawdzanie.** Wczytywanie przychodzących faktur UBL i CII, łącznie z Peppol BIS Billing 3.0, do jednego modelu danych EN 16931, który obejmuje nagłówek (numer, data, rodzaj, waluta), sprzedawcę i nabywcę, pozycje, zestawienie VAT i sumy, oraz sprawdzanie ich względem reguł normy, niemieckich reguł XRechnung i reguł Peppol BIS. Testy używają oficjalnych przykładowych faktur, każdej z oczekiwanym wynikiem.
2. **Tworzenie i wysyłanie.** Zamiana faktury obop w plik XRechnung lub Factur-X albo, dla klientów ze Szwecji, w plik Peppol BIS Billing 3.0, i wysłanie go e-mailem lub jako pliku.

Wysyłanie przez sieć Peppol przyjdzie później, w drugiej fali planu. Będzie się odbywać przez certyfikowany punkt dostępowy wybrany przez firmę, z jej własnymi danymi dostępowymi. SpaceCorps nie będzie prowadzić punktu dostępowego.

Jak wszystko w obop, każdy krok będzie poleceniem, które działa tak samo w CLI, w REST API i przez MCP, więc agent AI może odczytać przychodzącą fakturę albo przygotować wychodzącą równie łatwo jak człowiek.

## Terminy

Pakiet niemiecki ma docelową datę wersji stabilnej 2027-09-30, trzy miesiące przed tym, jak obowiązek obejmie wszystkich. Daty wersji alfa (2027-03-31) i beta (2027-06-30) są propozycjami. Pakiet nazywa się stabilnym dopiero wtedy, gdy jego przypadki testowe przechodzą, a księgowy go zatwierdził. Jeśli tak się nie stało, pozostaje w wersji beta, a data się nie zmienia.

## Co dalej

Pierwszym krokiem jest czytnik: faktury UBL, łącznie z Peppol BIS, wczytywane do modelu EN 16931 i testowane na oficjalnych przykładach. Potem CII, a następnie sprawdzanie. Następny wpis pokaże czytnik w działaniu na oficjalnych przykładowych fakturach.

Możesz śledzić pracę w [repozytorium](https://github.com/SpaceCorps/obop), gdzie [plan pakietu niemieckiego](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) i stojące za nim [decyzje](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) są publiczne.

## Źródła

(pobrane 2026-11-25)

- [§ 14 UStG, Ausstellung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 14b UStG, Aufbewahrung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14b.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 27 UStG, Allgemeine Übergangsvorschriften (Absatz 38)](https://www.gesetze-im-internet.de/ustg_1980/__27.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 33 UStDV, Rechnungen über Kleinbeträge](https://www.gesetze-im-internet.de/ustdv_1980/__33.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/), Sveriges riksdag
- [Regler för e-handel och e-faktura](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), Upphandlingsmyndigheten
- [E-faktura från statliga myndigheter](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/), Upphandlingsmyndigheten
- [XRechnung](https://xeinkauf.de/xrechnung/), XStandards Einkauf (KoSIT)
- [ZUGFeRD/Factur-X](https://www.ferd-net.de/standards/zugferd), FeRD
- [ZUGFeRD 2.5.2 english](https://www.ferd-net.de/publikationen-produkte/publikationen/detailseite/zugferd-252-english), FeRD
- [Peppol BIS Billing 3.0](https://docs.peppol.eu/poacc/billing/3.0/), OpenPeppol
- [docs/LOCALIZATION.md](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md), obop
- [docs/DECISIONS.md](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md), obop
