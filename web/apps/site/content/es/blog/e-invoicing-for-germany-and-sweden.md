---
title: "Factura electrónica para Alemania y Suecia: qué estamos construyendo"
description: "Alemania hace obligatoria la factura electrónica estructurada para todas las empresas a partir de 2028, y los compradores públicos suecos ya las reciben. Así es como obop planea leerlas, comprobarlas y escribirlas, y esto es lo que ya existe."
date: "2026-11-25"
author: "Abhinav Singh"
---

A partir del 2028-01-01, toda empresa establecida en Alemania debe enviar sus facturas a otras empresas en Alemania como facturas electrónicas estructuradas. Un PDF ya no basta. Esta es la razón principal por la que Alemania es el primer país de obop, y la factura electrónica es el trabajo que acabamos de empezar. Suecia exige la misma norma europea en las facturas a compradores públicos. Esta entrada explica por qué hace falta este trabajo, qué exige la ley en Alemania y en Suecia, qué tiene obop hoy y qué esperar de él, cómo se construye una factura electrónica, cómo serán las dos pantallas principales, las fechas y lo que viene.

## Por qué hace falta

Una aplicación de facturación que no puede escribir una factura electrónica deja de servir en Alemania para las facturas a otras empresas. A partir del 2027-01-01 eso se aplica a las empresas con un volumen de negocio superior a 800.000 EUR en el año anterior, y a partir del 2028-01-01 a todas las empresas. Toda empresa ya debe poder recibir facturas electrónicas, desde el 2025-01-01. Sin factura electrónica, una empresa alemana no podría usar obop para facturar a otras empresas después de 2027 o 2028, según su tamaño.

En Suecia la misma norma europea se aplica a las facturas a compradores públicos. Una empresa sueca que vende a un municipio, a una región o a un organismo del Estado la necesita hoy.

Muchas empresas aún no están preparadas. En un informe de 2026 de Quadient, un proveedor de software de factura electrónica, basado en más de 300 empresas B2B alemanas, el 89 % confiaba en cumplir los plazos, pero solo el 6 % cumplía plenamente los requisitos operativos.

## Qué exige la ley

La ley alemana (§ 14 UStG) define la factura electrónica como una factura emitida, enviada y recibida en un formato electrónico estructurado. Ese formato debe seguir la norma europea de facturación electrónica, EN 16931, o ser otro formato que ambas partes acuerden y que siga permitiendo su tratamiento electrónico. Las facturas deben conservarse ocho años, con su origen, su contenido y su legibilidad garantizados (§ 14b UStG). obop conservará sin cambios el archivo XML de una factura electrónica recibida. La obligación llega por etapas:

- **Desde el 2025-01-01:** toda empresa debe poder recibir facturas electrónicas.
- **A partir del 2027-01-01:** las empresas con un volumen de negocio superior a 800.000 EUR en el año anterior deben emitirlas.
- **A partir del 2028-01-01:** todas las empresas deben emitirlas.

Entre las excepciones están las facturas de hasta 250 EUR (§ 33 UStDV) y, hasta el final de 2027, los procedimientos EDI acordados con el destinatario (§ 27 Abs. 38 UStG).

Alemania acepta varios formatos. obop planea admitir tres: XRechnung, ZUGFeRD 2.5.2 (lo mismo que Factur-X 1.09.2, un PDF con el XML dentro) y Peppol BIS Billing 3.0. Detrás de estos formatos hay dos sintaxis XML, UBL y CII, y ambas llevan el mismo contenido EN 16931.

## Suecia

En Suecia la obligación cubre las facturas al sector público. La [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) entró en vigor el 2019-04-01. En las contrataciones iniciadas desde esa fecha, las facturas de un proveedor a un comprador público deben ser facturas electrónicas conformes a la norma europea EN 16931 (4 §), y los compradores públicos deben recibir y tramitar esas facturas (5 §). Proveedor y comprador pueden acordar otra norma. La ley no nombra ningún formato. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), responsable de la normativa correspondiente desde el 2026-07-01, recomienda Peppol BIS Billing 3, una implementación de EN 16931, y [los organismos del Estado deben admitirlo](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Los compradores públicos deben estar registrados en Peppol como destinatarios de facturas y poder recibir tanto UBL como CII.

## Qué tiene obop hoy

Hoy, la aplicación de Facturación de obop guarda las facturas de clientes y sus líneas, suma el total y da a cada factura contabilizada el siguiente número de una serie anual sin huecos. La contabiliza una persona; un agente de IA puede proponerlo. Lo siguiente que gana es la factura electrónica: leer, comprobar y escribir archivos EN 16931, con un desglose del IVA por tipo.

## Qué esperar de obop

Esto es lo que viene, paso a paso.

**Recepción.** Una persona, o un agente de IA que trabaje para ella, dará a obop un archivo de factura electrónica: un archivo XML o un PDF ZUGFeRD con el XML dentro. obop lo leerá y lo comprobará contra las reglas de EN 16931 y de su formato: que los totales cuadran y que están los campos obligatorios. También comprobará que la factura de proveedor no se haya registrado antes, por vendedor, número, importe y fecha. El resultado será un borrador de factura de proveedor. El propio archivo se conservará sin cambios, con su huella (un hash SHA-256).

**Envío.** Cuando una persona contabilice una factura, obop comprobará primero que sus totales son correctos y solo entonces le dará su número. A partir de la factura contabilizada generará un archivo XRechnung o ZUGFeRD para clientes alemanes, comprobará ese archivo contra las reglas, lo conservará sin cambios y lo enviará por correo electrónico o lo guardará como archivo. La lectura de facturas Peppol BIS Billing 3.0 forma parte del trabajo actual; su escritura para clientes suecos está prevista para más adelante. Una factura rechazada conservará su número; la corrección será una nota de abono y una factura nueva. El envío por la red Peppol llegará más tarde, en la segunda ola del plan. Irá a través de un punto de acceso certificado que elija la empresa, con sus propias credenciales. SpaceCorps no operará ningún punto de acceso.

**Agentes de IA.** Un agente de IA podrá leer una factura electrónica y el resultado de sus comprobaciones, y preparar a partir de ella un borrador de factura de proveedor. Contabilizar una factura de cliente o una factura de proveedor seguirá en manos de las personas: un agente lo propondrá y una persona lo contabilizará. Cuando un agente pida enviar una factura electrónica, el envío esperará a que una persona lo confirme. Como todo en obop, cada paso será un comando que funciona igual en la CLI, la API REST y MCP, de modo que un agente de IA pueda leer una factura entrante o preparar una saliente con la misma facilidad que una persona.

## Cómo se construye una factura electrónica

- **EN 16931** es el contenido: qué datos lleva una factura y qué significan. Cada uno tiene un número, como BT-1 para el número de factura o BT-2 para la fecha de emisión.
- **UBL y CII** son dos formatos XML que pueden llevar ese contenido. Ambos están permitidos, y un receptor debe admitir los dos.
- **XRechnung y Peppol BIS Billing 3.0** añaden encima las reglas de un país o de una red: qué campos son obligatorios y qué códigos se permiten. XRechnung es el alemán; Peppol BIS se usa en toda Europa y se recomienda en Suecia.
- **Correo electrónico, archivo y Peppol** son formas de transportar el archivo. El correo electrónico y el archivo no necesitan un contrato con nadie. Peppol es una red a la que te unes a través de un punto de acceso.

## Vistas previas del diseño

Hemos diseñado las dos pantallas que usarán las personas.

<figure class="mockup">
<div class="mockup-label">Vista previa del diseño</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">Factura electrónica entrante · rechnung-4711.pdf</span><span class="mockup-tag">ZUGFeRD · EN 16931</span></div>
<dl class="mockup-rows">
<div><dt>Vendedor</dt><dd>Müller Bürobedarf GmbH (TEST) · DE000000001</dd></div>
<div><dt>Número de factura</dt><dd>RE-2026-4711</dd></div>
<div><dt>Emitida · Vence</dt><dd>2026-11-12 · 2026-12-12</dd></div>
<div><dt>Base · IVA · A pagar</dt><dd>1000,00 € · 190,00 € · 1190,00 €</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Correcto: </span>Los totales cuadran</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Correcto: </span>No registrada antes</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Aviso: </span>El vendedor aún no es un contacto</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Crear borrador de factura de proveedor</span><span class="mockup-btn">Descartar</span></div>
</div>
<figcaption>Recepción: una factura electrónica recibida, después de que obop la haya leído y comprobado, antes de que una persona cree el borrador de factura de proveedor. Datos de ejemplo.</figcaption>
</figure>

<figure class="mockup">
<div class="mockup-label">Vista previa del diseño</div>
<div class="mockup-card">
<div class="mockup-head"><span class="mockup-title">INV-2026-00042 · Schmidt Logistik AG (TEST)</span><span class="mockup-tag">Contabilizada</span></div>
<dl class="mockup-rows">
<div><dt>Total con IVA</dt><dd>2380,00 €</dd></div>
<div><dt>Formato de factura electrónica</dt><dd>XRechnung (CII) · ZUGFeRD PDF</dd></div>
<div><dt>Referencia del comprador (BT-10)</dt><dd>PO-88213</dd></div>
</dl>
<ul class="mockup-checks">
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Correcto: </span>Se cumplen las reglas de EN 16931</span></li>
<li class="mockup-ok"><span class="mockup-icon" aria-hidden="true">✓</span><span><span class="sr-only">Correcto: </span>Cada línea tiene un impuesto y una categoría de IVA</span></li>
<li class="mockup-warn"><span class="mockup-icon" aria-hidden="true">!</span><span><span class="sr-only">Aviso: </span>Falta el número de IVA del comprador</span></li>
</ul>
<div class="mockup-actions"><span class="mockup-btn mockup-btn-primary">Descargar XRechnung</span><span class="mockup-btn">Descargar PDF ZUGFeRD</span></div>
</div>
<figcaption>Envío: una factura contabilizada con sus archivos de factura electrónica, comprobados contra las reglas antes de que alguien los descargue. Datos de ejemplo.</figcaption>
</figure>

## Las fechas

El paquete alemán tiene como fecha objetivo de su versión estable el 2027-09-30, tres meses antes de que la obligación se aplique a todos. Las fechas de la alfa (2027-03-31) y la beta (2027-06-30) son propuestas. El paquete solo se llama estable cuando sus casos de prueba pasan y un contable lo ha aprobado. Si eso no ha ocurrido, sigue en beta y la fecha se queda donde está.

## Lo que viene

El módulo de lectura está en desarrollo activo: ya lee la cabecera de cada factura UBL de nuestro conjunto de prueba de 119 archivos de ejemplo y rechaza los archivos demasiado grandes y el XML inseguro. Después leerá el vendedor y el comprador, las líneas, el desglose del IVA y los totales. Luego vendrá CII y después las comprobaciones.

Puedes seguir el trabajo en el [repositorio](https://github.com/SpaceCorps/obop), donde el [plan del paquete alemán](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) y las [decisiones](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) que hay detrás son públicos.

## Fuentes

(consultadas el 2026-11-25, y el 2026-11-28 las marcadas)

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
