---
title: "La facture électronique pour l'Allemagne et la Suède : ce que nous construisons"
description: "L'Allemagne rend la facture électronique structurée obligatoire pour toutes les entreprises à partir de 2028, et les acheteurs publics suédois en reçoivent déjà. Voici comment obop prévoit de les lire, de les vérifier et de les écrire, et ce qui existe aujourd'hui."
date: "2026-11-25"
---

À partir du 2028-01-01, toute entreprise établie en Allemagne doit envoyer ses factures aux autres entreprises en Allemagne sous forme de factures électroniques structurées. Un PDF ne suffit plus. C'est la raison principale pour laquelle l'Allemagne est le premier pays d'obop, et la facture électronique est le chantier que nous venons de commencer. La Suède exige la même norme européenne pour les factures adressées aux acheteurs publics. Cet article dit ce que la loi exige, ce qu'obop a aujourd'hui et ce que nous construisons.

## Ce que la loi exige

Le droit allemand (§ 14 UStG) définit la facture électronique comme une facture émise, transmise et reçue dans un format électronique structuré. Ce format doit suivre la norme européenne de facturation électronique, EN 16931, ou être un autre format convenu entre les deux parties qui reste exploitable électroniquement. Les factures doivent être conservées huit ans, avec leur origine, leur contenu et leur lisibilité garantis (§ 14b UStG). obop conserve sans modification le fichier XML d'une facture électronique reçue. L'obligation arrive par étapes :

- **Depuis le 2025-01-01 :** toute entreprise doit pouvoir recevoir des factures électroniques.
- **À partir du 2027-01-01 :** les entreprises dont le chiffre d'affaires de l'année précédente dépasse 800 000 EUR doivent en émettre.
- **À partir du 2028-01-01 :** toutes les entreprises doivent en émettre.

Les exceptions comprennent notamment les factures jusqu'à 250 EUR (§ 33 UStDV) et, jusqu'à la fin de 2027, les procédures EDI convenues avec le destinataire (§ 27 Abs. 38 UStG).

L'Allemagne accepte plusieurs formats. obop prévoit d'en prendre en charge trois : XRechnung, ZUGFeRD 2.5.2 (identique à Factur-X 1.09.2, un PDF qui contient le XML) et Peppol BIS Billing 3.0. Derrière ces formats se trouvent deux syntaxes XML, UBL et CII, qui portent toutes deux le même contenu EN 16931.

## La Suède

En Suède, l'obligation porte sur les factures adressées au secteur public. La [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) est entrée en vigueur le 2019-04-01. Pour les marchés lancés à partir de cette date, les factures d'un fournisseur à un acheteur public doivent être des factures électroniques conformes à la norme européenne EN 16931 (4 §), et les acheteurs publics doivent recevoir et traiter ces factures (5 §). Le fournisseur et l'acheteur peuvent convenir d'une autre norme. La loi ne désigne aucun format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), chargée de la réglementation correspondante depuis le 2026-07-01, recommande Peppol BIS Billing 3, une mise en œuvre d'EN 16931, et [les administrations de l'État doivent le prendre en charge](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Les acheteurs publics doivent être enregistrés dans Peppol comme destinataires de factures et pouvoir recevoir à la fois UBL et CII.

## Ce qu'obop a aujourd'hui

obop a une application Facturation. Elle tient les factures clients et leurs lignes, calcule le total et donne à chaque facture comptabilisée le numéro suivant d'une série annuelle sans trou. Un agent IA peut seulement proposer de comptabiliser une facture ; c'est une personne qui la comptabilise.

Facturation ne fait pas de facture électronique. Elle ne sait ni produire ni lire un fichier XRechnung, Factur-X ou Peppol BIS, et ne vérifie rien par rapport à EN 16931. La facture n'a pas encore non plus de ventilation de la TVA, que la norme exige.

## Ce que nous construisons

Le travail a deux moitiés. Nous commençons par la lecture, car depuis 2025 les entreprises doivent être prêtes à recevoir des factures électroniques :

1. **Lire et vérifier.** Lire les factures UBL et CII entrantes, Peppol BIS Billing 3.0 compris, dans un modèle de données EN 16931 unique qui couvre l'en-tête (numéro, date, type, devise), le vendeur et l'acheteur, les lignes, la ventilation de la TVA et les totaux, et les vérifier par rapport aux règles de la norme, aux règles allemandes XRechnung et aux règles Peppol BIS. Les tests utilisent les factures d'exemple officielles, chacune avec le résultat attendu.
2. **Écrire et envoyer.** Transformer une facture obop en fichier XRechnung ou Factur-X, ou en fichier Peppol BIS Billing 3.0 pour les clients suédois, et l'envoyer par e-mail ou sous forme de fichier.

L'envoi par le réseau Peppol viendra plus tard, dans la deuxième vague du plan. Il passera par un point d'accès certifié choisi par l'entreprise, avec ses propres identifiants. SpaceCorps n'exploitera pas de point d'accès.

Comme tout dans obop, chaque étape sera une commande qui fonctionne de la même façon dans la CLI, l'API REST et MCP, si bien qu'un agent IA peut lire une facture entrante ou en préparer une sortante aussi facilement qu'une personne.

## Les dates

Le paquet allemand a pour date cible de sa version stable le 2027-09-30, trois mois avant que l'obligation ne s'applique à tous. Les dates de l'alpha (2027-03-31) et de la bêta (2027-06-30) sont des propositions. Le paquet n'est dit stable que lorsque ses cas de test passent et qu'un comptable l'a validé. Sinon, il reste en bêta et la date ne bouge pas.

## La suite

La première étape est le lecteur : des factures UBL, Peppol BIS compris, lues dans le modèle EN 16931 et testées avec les exemples officiels. Viendront ensuite CII, puis les vérifications. Le prochain article montrera le lecteur à l'œuvre sur les factures d'exemple officielles.

Vous pouvez suivre le travail dans le [dépôt](https://github.com/SpaceCorps/obop), où le [plan du paquet allemand](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) et les [décisions](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) qui le fondent sont publics.

## Sources

(consultées le 2026-11-25)

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
