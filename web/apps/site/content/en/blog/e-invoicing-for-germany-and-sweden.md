---
title: "E-invoicing for Germany and Sweden: what we are building"
description: "Germany makes structured e-invoices compulsory for every business from 2028, and Swedish public buyers already receive them. This is how obop plans to read, check and write them, and what exists today."
date: "2026-11-25"
author: "Abhinav Singh"
---

From 2028-01-01 every business established in Germany must send its invoices to other businesses in Germany as structured e-invoices. A PDF is no longer enough. This is the main reason Germany is obop's first country, and e-invoicing is the work we have started now. Sweden asks for the same European standard on invoices to public buyers. This post says why the work is needed, what the law asks for in Germany and Sweden, what obop has today and what to expect from it, how an e-invoice is built, what the two main screens will look like, the dates and what comes next.

## Why this is needed

An invoicing app that cannot write an e-invoice stops being usable in Germany for invoices to other businesses. From 2027-01-01 that applies to businesses with more than EUR 800,000 turnover in the previous year, and from 2028-01-01 to every business. Every business must already be able to receive e-invoices, since 2025-01-01. Without e-invoicing, a German business could not use obop to invoice other businesses after 2027 or 2028, depending on its size.

In Sweden the same European standard applies to invoices to public buyers. A Swedish business that sells to a municipality, a region or a state agency needs it today.

Many businesses are not ready yet. In a 2026 report by Quadient, a vendor of e-invoicing software, based on more than 300 German B2B businesses, 89 percent were confident they would meet the deadlines, but only 6 percent had fully met the operational requirements.

## What the law asks for

German law (§ 14 UStG) defines an e-invoice as an invoice issued, sent and received in a structured electronic format. That format must follow the European standard for e-invoicing, EN 16931, or be another format both sides agree on that can still be processed electronically. Invoices must be kept for eight years, with their origin, content and readability assured (§ 14b UStG). obop will keep a received e-invoice's XML file unchanged. The duty comes in steps:

- **Since 2025-01-01:** every business must be able to receive e-invoices.
- **From 2027-01-01:** businesses with more than EUR 800,000 turnover in the previous year must issue them.
- **From 2028-01-01:** every business must issue them.

Exceptions include invoices up to EUR 250 (§ 33 UStDV) and, until the end of 2027, EDI procedures agreed with the recipient (§ 27 Abs. 38 UStG).

Germany accepts several formats. obop plans to handle three of them: XRechnung, ZUGFeRD 2.5.2 (the same as Factur-X 1.09.2, a PDF with the XML inside) and Peppol BIS Billing 3.0. Behind these formats are two XML syntaxes, UBL and CII, and both carry the same EN 16931 content.

## Sweden

In Sweden the duty covers invoices to the public sector. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) came into force on 2019-04-01. For procurements begun from that date, a supplier's invoices to a public buyer must be electronic invoices that follow the European standard, EN 16931 (4 §), and public buyers must receive and process such invoices (5 §). Supplier and buyer may agree on another standard. The law does not name a format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), responsible for the related regulations since 2026-07-01, recommends Peppol BIS Billing 3, an implementation of EN 16931, and [state agencies must support it](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Public buyers must be registered in Peppol as invoice recipients and must be able to receive both UBL and CII.

## What obop has today

Today, obop's Invoicing app keeps customer invoices and their lines, adds up the total and gives each posted invoice the next number of a gapless yearly series. A person posts; an AI agent may propose it. E-invoicing is what it gains next: reading, checking and writing EN 16931 files, with a VAT breakdown per rate.

## What to expect from obop

Here is what's coming, step by step.

**Receiving.** A person, or an AI agent working for one, will give obop an e-invoice file: an XML file, or a ZUGFeRD PDF with the XML inside. obop will read it and check it against the rules of EN 16931 and of its format: that the totals add up and that the required fields are there. It will also check that the bill has not been booked before, by seller, number, amount and date. The result will be a draft bill. The file itself will be kept unchanged, with its fingerprint (a SHA-256 hash).

**Sending.** When a person posts an invoice, obop will first check that its totals are right, and only then give it its number. From the posted invoice it will make an XRechnung or ZUGFeRD file for German customers, check that file against the rules, keep it unchanged and send it by email or save it as a file. Reading Peppol BIS Billing 3.0 invoices is part of the current work; writing them for Swedish customers is planned for later. A rejected invoice will keep its number; the fix will be a credit note and a new invoice. Sending through the Peppol network comes later, in the second wave of the plan. It will go through a certified access point that the business chooses, with its own credentials. SpaceCorps will not run an access point.

**AI agents.** An AI agent will be able to read an e-invoice and the result of its checks, and to prepare a draft bill from it. Posting an invoice or a bill will stay with people: an agent will propose it, and a person will post. When an agent asks to send an e-invoice, the send will wait for a person to confirm it. As with everything in obop, each step will be a command that works the same way in the CLI, the REST API and MCP, so an AI agent can read an incoming invoice or prepare an outgoing one as easily as a person can.

## How an e-invoice is built

- **EN 16931** is the content: which facts an invoice carries and what they mean. Each one has a number, such as BT-1 for the invoice number or BT-2 for the issue date.
- **UBL and CII** are two XML formats that can carry that content. Both are allowed, and a receiver must handle both.
- **XRechnung and Peppol BIS Billing 3.0** add the rules of a country or a network on top: which fields are required, which codes are allowed. XRechnung is the German one; Peppol BIS is used across Europe and recommended in Sweden.
- **Email, file and Peppol** are ways to transport the file. Email and file need no contract with anyone. Peppol is a network you join through an access point.

## Design previews

We've designed the two screens people will use.

<figure class="mockup">
<div class="mockup-label">Design preview</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">Incoming e-invoice · rechnung-4711.pdf</span><span class="mockup-tag">ZUGFeRD · EN 16931</span></div>
<dl class="mockup-rows">
<div><dt>Seller</dt><dd>Müller Bürobedarf GmbH (TEST) · DE000000001</dd></div>
<div><dt>Invoice number</dt><dd>RE-2026-4711</dd></div>
<div><dt>Issued · Due</dt><dd>2026-11-12 · 2026-12-12</dd></div>
<div><dt>Net · VAT · Payable</dt><dd>€1,000.00 · €190.00 · €1,190.00</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Passed: </span>Totals add up</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Passed: </span>Not booked before</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Warning: </span>Seller is not a contact yet</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Create draft bill</span><span class="mockup-btn">Discard</span></div>
</div>
<figcaption>Receiving: a received e-invoice after obop has read and checked it, before a person creates the draft bill. Sample data.</figcaption>
</figure>

<figure class="mockup">
<div class="mockup-label">Design preview</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">INV-2026-00042 · Schmidt Logistik AG (TEST)</span><span class="mockup-tag">Posted</span></div>
<dl class="mockup-rows">
<div><dt>Total incl. VAT</dt><dd>€2,380.00</dd></div>
<div><dt>E-invoice format</dt><dd>XRechnung (CII) · ZUGFeRD PDF</dd></div>
<div><dt>Buyer reference (BT-10)</dt><dd>PO-88213</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Passed: </span>EN 16931 rules pass</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Passed: </span>Every line has a tax and a VAT category</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Warning: </span>Buyer VAT id missing</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Download XRechnung</span><span class="mockup-btn">Download ZUGFeRD PDF</span></div>
</div>
<figcaption>Sending: a posted invoice with its e-invoice files, checked against the rules before anyone downloads them. Sample data.</figcaption>
</figure>

## The dates

The German pack has a target date of 2027-09-30 for its stable release, three months before the duty applies to everyone. The alpha (2027-03-31) and beta (2027-06-30) dates are proposals. The pack is called stable only when its test cases pass and an accountant has signed it off. If that has not happened, it stays beta and the date stays where it is.

## What comes next

The reader is in active development: it already reads the header of every UBL invoice in our test set of 119 sample files, and refuses oversized files and unsafe XML. Next it reads the seller and buyer, the lines, the VAT breakdown and the totals. CII follows, then the checks.

You can follow the work in the [repository](https://github.com/SpaceCorps/obop), where the [plan for the German pack](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) and the [decisions](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) behind it are public.

## Sources

(retrieved 2026-11-25, and 2026-11-28 for the ones marked)

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
