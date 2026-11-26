---
title: "Factura electrónica para Alemania y Suecia: qué estamos construyendo"
description: "Alemania hace obligatoria la factura electrónica estructurada para todas las empresas a partir de 2028, y los compradores públicos suecos ya las reciben. Así es como obop planea leerlas, comprobarlas y escribirlas, y esto es lo que ya existe."
date: "2026-11-25"
---

A partir del 2028-01-01, toda empresa establecida en Alemania debe enviar sus facturas a otras empresas en Alemania como facturas electrónicas estructuradas. Un PDF ya no basta. Esta es la razón principal por la que Alemania es el primer país de obop, y la factura electrónica es el trabajo que acabamos de empezar. Suecia exige la misma norma europea en las facturas a compradores públicos. Esta entrada explica qué exige la ley, qué tiene obop hoy y qué estamos construyendo.

## Qué exige la ley

La ley alemana (§ 14 UStG) define la factura electrónica como una factura emitida, enviada y recibida en un formato electrónico estructurado. Ese formato debe seguir la norma europea de facturación electrónica, EN 16931, o ser otro formato que ambas partes acuerden y que siga permitiendo su tratamiento electrónico. Las facturas deben conservarse ocho años, con su origen, su contenido y su legibilidad garantizados (§ 14b UStG). obop conserva sin cambios el archivo XML de una factura electrónica recibida. La obligación llega por etapas:

- **Desde el 2025-01-01:** toda empresa debe poder recibir facturas electrónicas.
- **A partir del 2027-01-01:** las empresas con un volumen de negocio superior a 800.000 EUR en el año anterior deben emitirlas.
- **A partir del 2028-01-01:** todas las empresas deben emitirlas.

Entre las excepciones están las facturas de hasta 250 EUR (§ 33 UStDV) y, hasta el final de 2027, los procedimientos EDI acordados con el destinatario (§ 27 Abs. 38 UStG).

Alemania acepta varios formatos. obop planea admitir tres: XRechnung, ZUGFeRD 2.5.2 (lo mismo que Factur-X 1.09.2, un PDF con el XML dentro) y Peppol BIS Billing 3.0. Detrás de estos formatos hay dos sintaxis XML, UBL y CII, y ambas llevan el mismo contenido EN 16931.

## Suecia

En Suecia la obligación cubre las facturas al sector público. La [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) entró en vigor el 2019-04-01. En las contrataciones iniciadas desde esa fecha, las facturas de un proveedor a un comprador público deben ser facturas electrónicas conformes a la norma europea EN 16931 (4 §), y los compradores públicos deben recibir y tramitar esas facturas (5 §). Proveedor y comprador pueden acordar otra norma. La ley no nombra ningún formato. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), responsable de la normativa correspondiente desde el 2026-07-01, recomienda Peppol BIS Billing 3, una implementación de EN 16931, y [los organismos del Estado deben admitirlo](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Los compradores públicos deben estar registrados en Peppol como destinatarios de facturas y poder recibir tanto UBL como CII.

## Qué tiene obop hoy

obop tiene una aplicación de Facturación. Guarda las facturas de clientes y sus líneas, suma el total y da a cada factura contabilizada el siguiente número de una serie anual sin huecos. Un agente de IA solo puede proponer contabilizar una factura; la contabiliza una persona.

Facturación no hace factura electrónica. No puede generar un archivo XRechnung, Factur-X o Peppol BIS, no puede leerlo y no comprueba nada contra EN 16931. La factura tampoco tiene todavía un desglose del IVA, y la norma lo exige.

## Qué estamos construyendo

El trabajo tiene dos mitades. Empezamos por la lectura, porque desde 2025 las empresas tienen que estar preparadas para recibir facturas electrónicas:

1. **Leer y comprobar.** Leer las facturas UBL y CII entrantes, Peppol BIS Billing 3.0 incluido, en un único modelo de datos EN 16931 que cubre la cabecera (número, fecha, tipo, moneda), vendedor y comprador, líneas, el desglose del IVA y los totales, y comprobarlas contra las reglas de la norma, las reglas alemanas de XRechnung y las reglas de Peppol BIS. Las pruebas usan las facturas de ejemplo oficiales, cada una con el resultado que esperamos.
2. **Escribir y enviar.** Convertir una factura de obop en un archivo XRechnung o Factur-X, o en un archivo Peppol BIS Billing 3.0 para clientes suecos, y enviarlo por correo electrónico o como archivo.

El envío por la red Peppol llegará más tarde, en la segunda ola del plan. Irá a través de un punto de acceso certificado que elija la empresa, con sus propias credenciales. SpaceCorps no operará ningún punto de acceso.

Como todo en obop, cada paso será un comando que funciona igual en la CLI, la API REST y MCP, de modo que un agente de IA pueda leer una factura entrante o preparar una saliente con la misma facilidad que una persona.

## Las fechas

El paquete alemán tiene como fecha objetivo de su versión estable el 2027-09-30, tres meses antes de que la obligación se aplique a todos. Las fechas de la alfa (2027-03-31) y la beta (2027-06-30) son propuestas. El paquete solo se llama estable cuando sus casos de prueba pasan y un contable lo ha aprobado. Si eso no ha ocurrido, sigue en beta y la fecha se queda donde está.

## Lo que viene

El primer paso es el lector: facturas UBL, Peppol BIS incluido, leídas en el modelo EN 16931 y probadas con los ejemplos oficiales. Después vendrá CII y luego las comprobaciones. La próxima entrada mostrará el lector funcionando con las facturas de ejemplo oficiales.

Puedes seguir el trabajo en el [repositorio](https://github.com/SpaceCorps/obop), donde el [plan del paquete alemán](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) y las [decisiones](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) que hay detrás son públicos.

## Fuentes

(consultadas el 2026-11-25)

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
