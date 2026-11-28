---
title: "E-faktury dla Niemiec i Szwecji: co budujemy"
description: "Niemcy wprowadzają od 2028 roku obowiązek ustrukturyzowanych e-faktur dla każdej firmy, a szwedzcy zamawiający publiczni już je przyjmują. Tak obop zamierza je czytać, sprawdzać i tworzyć, a to istnieje już dziś."
date: "2026-11-25"
author: "Abhinav Singh"
---

Od 2028-01-01 każda firma z siedzibą w Niemczech musi wysyłać faktury do innych firm w Niemczech jako ustrukturyzowane e-faktury. PDF już nie wystarczy. To główny powód, dla którego Niemcy są pierwszym krajem obop, a e-faktury to praca, którą właśnie zaczęliśmy. Szwecja wymaga tej samej normy europejskiej dla faktur do zamawiających publicznych. Ten wpis mówi, dlaczego ta praca jest potrzebna, czego wymaga prawo w Niemczech i w Szwecji, co obop ma dziś i czego można się po nim spodziewać, jak zbudowana jest e-faktura, jak będą wyglądać dwa główne ekrany, jakie są terminy i co dalej.

## Dlaczego to jest potrzebne

Aplikacja do fakturowania, która nie potrafi utworzyć e-faktury, przestaje nadawać się w Niemczech do faktur dla innych firm. Od 2027-01-01 dotyczy to firm z obrotem powyżej 800 000 EUR w poprzednim roku, a od 2028-01-01 każdej firmy. Każda firma już od 2025-01-01 musi być w stanie odbierać e-faktury. Bez e-faktur niemiecka firma nie mogłaby po 2027 lub 2028 roku, zależnie od swojej wielkości, fakturować innych firm w obop.

W Szwecji ta sama norma europejska obowiązuje dla faktur do zamawiających publicznych. Szwedzka firma, która sprzedaje gminie, regionowi albo urzędowi państwowemu, potrzebuje jej już dziś.

Wiele firm nie jest jeszcze gotowych. W raporcie z 2026 roku firmy Quadient, dostawcy oprogramowania do e-faktur, opartym na ponad 300 niemieckich firmach B2B, 89 procent było pewnych, że dotrzyma terminów, ale tylko 6 procent w pełni spełniało wymagania operacyjne.

## Czego wymaga prawo

Prawo niemieckie (§ 14 UStG) definiuje e-fakturę jako fakturę wystawioną, przesłaną i odebraną w ustrukturyzowanym formacie elektronicznym. Ten format musi być zgodny z europejską normą fakturowania elektronicznego EN 16931 albo być innym formatem uzgodnionym przez obie strony, który nadal pozwala na elektroniczne przetwarzanie. Faktury trzeba przechowywać przez osiem lat, z zapewnioną autentycznością pochodzenia, integralnością treści i czytelnością (§ 14b UStG). obop będzie przechowywać plik XML odebranej e-faktury bez zmian. Obowiązek wchodzi etapami:

- **Od 2025-01-01:** każda firma musi być w stanie odbierać e-faktury.
- **Od 2027-01-01:** firmy z obrotem powyżej 800 000 EUR w poprzednim roku muszą je wystawiać.
- **Od 2028-01-01:** każda firma musi je wystawiać.

Wyjątki obejmują m.in. faktury do 250 EUR (§ 33 UStDV) oraz, do końca 2027 roku, procedury EDI uzgodnione z odbiorcą (§ 27 Abs. 38 UStG).

Niemcy akceptują kilka formatów. obop planuje obsłużyć trzy z nich: XRechnung, ZUGFeRD 2.5.2 (to samo co Factur-X 1.09.2, PDF z XML w środku) i Peppol BIS Billing 3.0. Za tymi formatami stoją dwie składnie XML, UBL i CII, i obie niosą tę samą treść EN 16931.

## Szwecja

W Szwecji obowiązek dotyczy faktur do sektora publicznego. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) weszła w życie 2019-04-01. W zamówieniach wszczętych od tego dnia faktury dostawcy do zamawiającego publicznego muszą być fakturami elektronicznymi zgodnymi z europejską normą EN 16931 (4 §), a zamawiający publiczni muszą takie faktury odbierać i przetwarzać (5 §). Dostawca i zamawiający mogą uzgodnić inną normę. Ustawa nie wskazuje formatu. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), odpowiedzialny od 2026-07-01 za powiązane przepisy, zaleca Peppol BIS Billing 3, implementację EN 16931, a [urzędy państwowe muszą go obsługiwać](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Zamawiający publiczni muszą być zarejestrowani w Peppol jako odbiorcy faktur i muszą móc odbierać zarówno UBL, jak i CII.

## Co obop ma dziś

Dziś aplikacja Fakturowanie w obop przechowuje faktury dla klientów i ich pozycje, sumuje kwotę i nadaje każdej zaksięgowanej fakturze kolejny numer z ciągłej rocznej serii. Księguje człowiek; agent AI może to zaproponować. Następnym krokiem są e-faktury: czytanie, sprawdzanie i tworzenie plików EN 16931, z zestawieniem VAT według stawek.

## Czego spodziewać się po obop

Oto, co nadchodzi, krok po kroku.

**Odbiór.** Człowiek albo działający w jego imieniu agent AI przekaże obop plik e-faktury: plik XML albo PDF ZUGFeRD z XML w środku. obop go odczyta i sprawdzi względem reguł EN 16931 i reguł danego formatu: czy sumy się zgadzają i czy wymagane pola są obecne. Sprawdzi też, czy faktura zakupowa nie została już wcześniej zaksięgowana, po sprzedawcy, numerze, kwocie i dacie. Wynikiem będzie szkic faktury zakupowej. Sam plik zostanie zachowany bez zmian, razem z jego odciskiem (skrótem SHA-256).

**Wysyłanie.** Gdy człowiek zaksięguje fakturę, obop najpierw sprawdzi, czy jej sumy są poprawne, i dopiero wtedy nada jej numer. Z zaksięgowanej faktury utworzy plik XRechnung lub ZUGFeRD dla klientów z Niemiec, sprawdzi ten plik względem reguł, zachowa go bez zmian i wyśle e-mailem albo zapisze jako plik. Odczyt faktur Peppol BIS Billing 3.0 jest częścią obecnych prac; ich tworzenie dla klientów ze Szwecji jest zaplanowane na później. Odrzucona faktura zachowa swój numer; poprawką będzie faktura korygująca i nowa faktura. Wysyłanie przez sieć Peppol przyjdzie później, w drugiej fali planu. Będzie się odbywać przez certyfikowany punkt dostępowy wybrany przez firmę, z jej własnymi danymi dostępowymi. SpaceCorps nie będzie prowadzić punktu dostępowego.

**Agenci AI.** Agent AI będzie mógł odczytać e-fakturę i wynik jej sprawdzenia oraz przygotować z niej szkic faktury zakupowej. Księgowanie faktury sprzedażowej lub zakupowej pozostanie w rękach ludzi: agent je zaproponuje, a człowiek zaksięguje. Gdy agent poprosi o wysłanie e-faktury, wysyłka zaczeka, aż człowiek ją potwierdzi. Jak wszystko w obop, każdy krok będzie poleceniem, które działa tak samo w CLI, w REST API i przez MCP, więc agent AI może odczytać przychodzącą fakturę albo przygotować wychodzącą równie łatwo jak człowiek.

## Jak zbudowana jest e-faktura

- **EN 16931** to treść: jakie informacje niesie faktura i co one znaczą. Każda ma numer, na przykład BT-1 dla numeru faktury albo BT-2 dla daty wystawienia.
- **UBL i CII** to dwa formaty XML, które mogą nieść tę treść. Oba są dozwolone, a odbiorca musi obsługiwać oba.
- **XRechnung i Peppol BIS Billing 3.0** dodają do tego reguły kraju lub sieci: które pola są wymagane, które kody są dozwolone. XRechnung to reguły niemieckie; Peppol BIS jest używany w całej Europie i zalecany w Szwecji.
- **E-mail, plik i Peppol** to sposoby przesłania pliku. E-mail i plik nie wymagają umowy z nikim. Peppol to sieć, do której dołącza się przez punkt dostępowy.

## Podglądy projektu

Zaprojektowaliśmy dwa ekrany, z których będą korzystać ludzie.

<figure class="mockup">
<div class="mockup-label">Podgląd projektu</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">Przychodząca e-faktura · rechnung-4711.pdf</span><span class="mockup-tag">ZUGFeRD · EN 16931</span></div>
<dl class="mockup-rows">
<div><dt>Sprzedawca</dt><dd>Müller Bürobedarf GmbH (TEST) · DE000000001</dd></div>
<div><dt>Numer faktury</dt><dd>RE-2026-4711</dd></div>
<div><dt>Wystawiona · Termin płatności</dt><dd>2026-11-12 · 2026-12-12</dd></div>
<div><dt>Netto · VAT · Do zapłaty</dt><dd>1000,00 € · 190,00 € · 1190,00 €</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Zaliczone: </span>Sumy się zgadzają</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Zaliczone: </span>Nie zaksięgowano wcześniej</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Ostrzeżenie: </span>Sprzedawca nie jest jeszcze kontaktem</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Utwórz szkic faktury zakupowej</span><span class="mockup-btn">Odrzuć</span></div>
</div>
<figcaption>Odbiór: odebrana e-faktura po odczytaniu i sprawdzeniu przez obop, zanim człowiek utworzy szkic faktury zakupowej. Dane przykładowe.</figcaption>
</figure>

<figure class="mockup">
<div class="mockup-label">Podgląd projektu</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">INV-2026-00042 · Schmidt Logistik AG (TEST)</span><span class="mockup-tag">Zaksięgowana</span></div>
<dl class="mockup-rows">
<div><dt>Razem z VAT</dt><dd>2380,00 €</dd></div>
<div><dt>Format e-faktury</dt><dd>XRechnung (CII) · ZUGFeRD PDF</dd></div>
<div><dt>Referencja nabywcy (BT-10)</dt><dd>PO-88213</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Zaliczone: </span>Reguły EN 16931 spełnione</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Zaliczone: </span>Każda pozycja ma podatek i kategorię VAT</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Ostrzeżenie: </span>Brak numeru VAT nabywcy</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Pobierz XRechnung</span><span class="mockup-btn">Pobierz PDF ZUGFeRD</span></div>
</div>
<figcaption>Wysyłka: zaksięgowana faktura z plikami e-faktury, sprawdzonymi względem reguł, zanim ktokolwiek je pobierze. Dane przykładowe.</figcaption>
</figure>

## Terminy

Pakiet niemiecki ma docelową datę wersji stabilnej 2027-09-30, trzy miesiące przed tym, jak obowiązek obejmie wszystkich. Daty wersji alfa (2027-03-31) i beta (2027-06-30) są propozycjami. Pakiet nazywa się stabilnym dopiero wtedy, gdy jego przypadki testowe przechodzą, a księgowy go zatwierdził. Jeśli tak się nie stało, pozostaje w wersji beta, a data się nie zmienia.

## Co dalej

Moduł odczytu jest w aktywnym rozwoju: już odczytuje nagłówek każdej faktury UBL z naszego zestawu testowego 119 przykładowych plików i odrzuca zbyt duże pliki oraz niebezpieczny XML. Następnie będzie odczytywać sprzedawcę i nabywcę, pozycje, zestawienie VAT i sumy. Potem CII, a następnie sprawdzanie.

Możesz śledzić pracę w [repozytorium](https://github.com/SpaceCorps/obop), gdzie [plan pakietu niemieckiego](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) i stojące za nim [decyzje](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) są publiczne.

## Źródła

(pobrane 2026-11-25, a oznaczone 2026-11-28)

- [§ 14 UStG, Ausstellung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 14b UStG, Aufbewahrung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14b.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 27 UStG, Allgemeine Übergangsvorschriften (Absatz 38)](https://www.gesetze-im-internet.de/ustg_1980/__27.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 33 UStDV, Rechnungen über Kleinbeträge](https://www.gesetze-im-internet.de/ustdv_1980/__33.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [E-Rechnung Deutschland 2026: Die B2B-Halbzeitbilanz](https://www.quadient.com/de/ressourcen/e-rechnung-deutschland-bericht-2026), Quadient (2026-11-28)
- [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/), Sveriges riksdag
- [Regler för e-handel och e-faktura](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), Upphandlingsmyndigheten
- [E-faktura från statliga myndigheter](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/), Upphandlingsmyndigheten
- [XRechnung](https://xeinkauf.de/xrechnung/), XStandards Einkauf (KoSIT)
- [ZUGFeRD/Factur-X](https://www.ferd-net.de/standards/zugferd), FeRD
- [ZUGFeRD 2.5.2 english](https://www.ferd-net.de/publikationen-produkte/publikationen/detailseite/zugferd-252-english), FeRD
- [Peppol BIS Billing 3.0](https://docs.peppol.eu/poacc/billing/3.0/), OpenPeppol
- [docs/spec/finance.md (sections 4.6 and 5.7, the capability table and 'Agentic first')](https://github.com/SpaceCorps/obop/blob/main/docs/spec/finance.md), obop (2026-11-28)
- [docs/LOCALIZATION.md](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md), obop
- [docs/DECISIONS.md](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md), obop
