---
title: "La facture électronique pour l'Allemagne et la Suède : ce que nous construisons"
description: "L'Allemagne rend la facture électronique structurée obligatoire pour toutes les entreprises à partir de 2028, et les acheteurs publics suédois en reçoivent déjà. Voici comment obop prévoit de les lire, de les vérifier et de les écrire, et ce qui existe aujourd'hui."
date: "2026-11-25"
author: "Abhinav Singh"
---

À partir du 2028-01-01, toute entreprise établie en Allemagne doit envoyer ses factures aux autres entreprises en Allemagne sous forme de factures électroniques structurées. Un PDF ne suffit plus. C'est la raison principale pour laquelle l'Allemagne est le premier pays d'obop, et la facture électronique est le chantier que nous venons de commencer. La Suède exige la même norme européenne pour les factures adressées aux acheteurs publics. Cet article dit pourquoi ce travail est nécessaire, ce que la loi exige en Allemagne et en Suède, ce qu'obop a aujourd'hui et ce qu'on peut en attendre, comment une facture électronique est construite, à quoi ressembleront les deux écrans principaux, les dates et la suite.

## Pourquoi c'est nécessaire

Une application de facturation qui ne sait pas écrire de facture électronique cesse d'être utilisable en Allemagne pour les factures adressées à d'autres entreprises. À partir du 2027-01-01, cela vaut pour les entreprises dont le chiffre d'affaires de l'année précédente dépasse 800 000 EUR, et à partir du 2028-01-01 pour toutes les entreprises. Depuis le 2025-01-01, toute entreprise doit déjà pouvoir recevoir des factures électroniques. Sans facture électronique, une entreprise allemande ne pourrait plus utiliser obop pour facturer d'autres entreprises après 2027 ou 2028, selon sa taille.

En Suède, la même norme européenne s'applique aux factures adressées aux acheteurs publics. Une entreprise suédoise qui vend à une commune, à une région ou à une administration de l'État en a besoin dès aujourd'hui.

Beaucoup d'entreprises ne sont pas encore prêtes. Dans un rapport de 2026 de Quadient, un éditeur de logiciels de facture électronique, fondé sur plus de 300 entreprises B2B allemandes, 89 % se disaient confiantes de respecter les échéances, mais seulement 6 % remplissaient entièrement les exigences opérationnelles.

## Ce que la loi exige

Le droit allemand (§ 14 UStG) définit la facture électronique comme une facture émise, transmise et reçue dans un format électronique structuré. Ce format doit suivre la norme européenne de facturation électronique, EN 16931, ou être un autre format convenu entre les deux parties qui reste exploitable électroniquement. Les factures doivent être conservées huit ans, avec leur origine, leur contenu et leur lisibilité garantis (§ 14b UStG). obop conservera sans modification le fichier XML d'une facture électronique reçue. L'obligation arrive par étapes :

- **Depuis le 2025-01-01 :** toute entreprise doit pouvoir recevoir des factures électroniques.
- **À partir du 2027-01-01 :** les entreprises dont le chiffre d'affaires de l'année précédente dépasse 800 000 EUR doivent en émettre.
- **À partir du 2028-01-01 :** toutes les entreprises doivent en émettre.

Les exceptions comprennent notamment les factures jusqu'à 250 EUR (§ 33 UStDV) et, jusqu'à la fin de 2027, les procédures EDI convenues avec le destinataire (§ 27 Abs. 38 UStG).

L'Allemagne accepte plusieurs formats. obop prévoit d'en prendre en charge trois : XRechnung, ZUGFeRD 2.5.2 (identique à Factur-X 1.09.2, un PDF qui contient le XML) et Peppol BIS Billing 3.0. Derrière ces formats se trouvent deux syntaxes XML, UBL et CII, qui portent toutes deux le même contenu EN 16931.

## La Suède

En Suède, l'obligation porte sur les factures adressées au secteur public. La [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) est entrée en vigueur le 2019-04-01. Pour les marchés lancés à partir de cette date, les factures d'un fournisseur à un acheteur public doivent être des factures électroniques conformes à la norme européenne EN 16931 (4 §), et les acheteurs publics doivent recevoir et traiter ces factures (5 §). Le fournisseur et l'acheteur peuvent convenir d'une autre norme. La loi ne désigne aucun format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), chargée de la réglementation correspondante depuis le 2026-07-01, recommande Peppol BIS Billing 3, une mise en œuvre d'EN 16931, et [les administrations de l'État doivent le prendre en charge](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Les acheteurs publics doivent être enregistrés dans Peppol comme destinataires de factures et pouvoir recevoir à la fois UBL et CII.

## Ce qu'obop a aujourd'hui

Aujourd'hui, l'application Facturation d'obop tient les factures clients et leurs lignes, calcule le total et donne à chaque facture comptabilisée le numéro suivant d'une série annuelle sans trou. C'est une personne qui comptabilise ; un agent IA peut le proposer. La facture électronique est ce qu'elle gagne ensuite : lire, vérifier et écrire des fichiers EN 16931, avec une ventilation de la TVA par taux.

## Ce qu'on peut attendre d'obop

Voici ce qui arrive, étape par étape.

**La réception.** Une personne, ou un agent IA qui travaille pour elle, donnera à obop un fichier de facture électronique : un fichier XML, ou un PDF ZUGFeRD qui contient le XML. obop le lira et le vérifiera par rapport aux règles d'EN 16931 et de son format : que les totaux s'additionnent et que les champs obligatoires sont présents. Il vérifiera aussi que la facture fournisseur n'a pas déjà été enregistrée, d'après le vendeur, le numéro, le montant et la date. Le résultat sera une facture fournisseur en brouillon. Le fichier lui-même sera conservé sans modification, avec son empreinte (un hachage SHA-256).

**L'envoi.** Quand une personne comptabilise une facture, obop vérifiera d'abord que ses totaux sont justes, et seulement ensuite lui donnera son numéro. À partir de la facture comptabilisée, il produira un fichier XRechnung ou ZUGFeRD pour les clients allemands, vérifiera ce fichier par rapport aux règles, le conservera sans modification et l'enverra par e-mail ou l'enregistrera sous forme de fichier. La lecture des factures Peppol BIS Billing 3.0 fait partie du travail en cours ; leur écriture pour les clients suédois est prévue plus tard. Une facture rejetée gardera son numéro ; la correction sera un avoir suivi d'une nouvelle facture. L'envoi par le réseau Peppol viendra plus tard, dans la deuxième vague du plan. Il passera par un point d'accès certifié choisi par l'entreprise, avec ses propres identifiants. SpaceCorps n'exploitera pas de point d'accès.

**Les agents IA.** Un agent IA pourra lire une facture électronique et le résultat de ses vérifications, et en préparer une facture fournisseur en brouillon. La comptabilisation d'une facture client ou fournisseur restera aux personnes : un agent la proposera, et une personne la comptabilisera. Quand un agent demandera d'envoyer une facture électronique, l'envoi attendra qu'une personne le confirme. Comme tout dans obop, chaque étape sera une commande qui fonctionne de la même façon dans la CLI, l'API REST et MCP, si bien qu'un agent IA pourra lire une facture entrante ou en préparer une sortante aussi facilement qu'une personne.

## Comment une facture électronique est construite

- **EN 16931** est le contenu : quelles informations une facture porte et ce qu'elles signifient. Chacune a un numéro, par exemple BT-1 pour le numéro de facture ou BT-2 pour la date d'émission.
- **UBL et CII** sont deux formats XML qui peuvent porter ce contenu. Les deux sont autorisés, et un destinataire doit prendre en charge les deux.
- **XRechnung et Peppol BIS Billing 3.0** ajoutent par-dessus les règles d'un pays ou d'un réseau : quels champs sont obligatoires, quels codes sont autorisés. XRechnung est celui de l'Allemagne ; Peppol BIS est utilisé dans toute l'Europe et recommandé en Suède.
- **L'e-mail, le fichier et Peppol** sont des moyens de transporter le fichier. L'e-mail et le fichier ne demandent de contrat avec personne. Peppol est un réseau que l'on rejoint par un point d'accès.

## Aperçus de conception

Nous avons conçu les deux écrans que les gens utiliseront.

<figure class="mockup">
<div class="mockup-label">Aperçu de conception</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">Facture électronique reçue · rechnung-4711.pdf</span><span class="mockup-tag">ZUGFeRD · EN 16931</span></div>
<dl class="mockup-rows">
<div><dt>Vendeur</dt><dd>Müller Bürobedarf GmbH (TEST) · DE000000001</dd></div>
<div><dt>Numéro de facture</dt><dd>RE-2026-4711</dd></div>
<div><dt>Émise · Échéance</dt><dd>2026-11-12 · 2026-12-12</dd></div>
<div><dt>HT · TVA · À payer</dt><dd>1 000,00 € · 190,00 € · 1 190,00 €</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Réussi : </span>Les totaux s'additionnent</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Réussi : </span>Pas déjà enregistrée</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Avertissement : </span>Le vendeur n'est pas encore un contact</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Créer une facture fournisseur en brouillon</span><span class="mockup-btn">Écarter</span></div>
</div>
<figcaption>Réception : une facture électronique reçue, une fois lue et vérifiée par obop, avant qu'une personne crée la facture fournisseur en brouillon. Données d'exemple.</figcaption>
</figure>

<figure class="mockup">
<div class="mockup-label">Aperçu de conception</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">INV-2026-00042 · Schmidt Logistik AG (TEST)</span><span class="mockup-tag">Comptabilisée</span></div>
<dl class="mockup-rows">
<div><dt>Total TTC</dt><dd>2 380,00 €</dd></div>
<div><dt>Format de facture électronique</dt><dd>XRechnung (CII) · ZUGFeRD PDF</dd></div>
<div><dt>Référence acheteur (BT-10)</dt><dd>PO-88213</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Réussi : </span>Règles d'EN 16931 respectées</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Réussi : </span>Chaque ligne a une taxe et une catégorie de TVA</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Avertissement : </span>Numéro de TVA de l'acheteur manquant</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Télécharger XRechnung</span><span class="mockup-btn">Télécharger le PDF ZUGFeRD</span></div>
</div>
<figcaption>Envoi : une facture comptabilisée et ses fichiers de facture électronique, vérifiés par rapport aux règles avant que quiconque les télécharge. Données d'exemple.</figcaption>
</figure>

## Les dates

Le paquet allemand a pour date cible de sa version stable le 2027-09-30, trois mois avant que l'obligation ne s'applique à tous. Les dates de l'alpha (2027-03-31) et de la bêta (2027-06-30) sont des propositions. Le paquet n'est dit stable que lorsque ses cas de test passent et qu'un comptable l'a validé. Sinon, il reste en bêta et la date ne bouge pas.

## La suite

Le module de lecture est en développement actif : il lit déjà l'en-tête de chaque facture UBL de notre jeu de test de 119 fichiers d'exemple, et refuse les fichiers trop volumineux et le XML non sûr. Ensuite, il lira le vendeur et l'acheteur, les lignes, la ventilation de la TVA et les totaux. Viendront ensuite CII, puis les vérifications.

Vous pouvez suivre le travail dans le [dépôt](https://github.com/SpaceCorps/obop), où le [plan du paquet allemand](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) et les [décisions](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) qui le fondent sont publics.

## Sources

(consultées le 2026-11-25, et le 2026-11-28 pour celles qui sont indiquées)

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
