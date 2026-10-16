---
title: "Brand"
description: "The obop brand in brief: name, logo with downloads, colour, typography, voice and accessibility. A public summary of the brand book."
lead: "How obop looks and sounds. The full brand book is in the repository and, rendered with the real components, in the product under Customize."
eyebrow: "Brand"
---

## Name

The name is **obop**: lowercase always, including at the start of a sentence. Never "Obop", "OBOP" or "the obop".

## Logo

The logo is a mark and a wordmark, drawn as SVG in one colour. The mark is a ring open on one side with a dot in the opening: one platform, the business in orbit. The wordmark is lowercase, geometric and monoline, drawn from the same circles, so it needs no font.

| Download                                       | Use on                              |
| ---------------------------------------------- | ----------------------------------- |
| [Full logo, ink](/brand/obop-logo.svg)         | Light backgrounds                   |
| [Full logo, white](/brand/obop-logo-white.svg) | Dark backgrounds                    |
| [Full logo, black](/brand/obop-logo-black.svg) | One-colour print                    |
| [Mark](/brand/obop-mark.svg)                   | Favicons, app badges                |
| [Wordmark](/brand/obop-wordmark.svg)           | Tight horizontal spaces             |
| [Favicon](/favicon.svg)                        | The mark in white on an accent tile |

Rules:

- One colour: ink on light, near-white on dark, the on-accent colour on an accent background, or pure black for print. Never gradients, outlines or shadows.
- Clear space of x on every side, where x is half the height of the logo.
- Minimum size: mark 16 px; full logo 20 px high.
- Do not stretch, rotate, recolour with effects, or place on low-contrast or busy backgrounds.

## Colour

Colours are OKLCH ramps with the same lightness ladder in every hue. Neutral is a cool grey. The accent is chosen by the person or administrator; the default is a violet, and its ramp is generated and checked for contrast: text on the accent fill at least 4.5:1, accent text on panels 4.5:1, fills against panels 3:1. Signals are success, warning, danger and info. Never rely on colour alone.

## Typography

Sans: **Geist Variable**. Mono: **Geist Mono Variable**. Both are open source (SIL OFL 1.1) and served from this site; there are no third-party font hosts. Numbers are tabular where they line up. Headings are weight 600 with tight tracking.

## Voice

Plain, specific, calm, honest, helpful, human. Sentence case. No exclamation marks. No invented customers, quotes or metrics anywhere.

| Do not                      | Do                                                            |
| --------------------------- | ------------------------------------------------------------- |
| Oops! Something went wrong! | Could not save the lead. Check your connection and try again. |
| No data.                    | No leads yet. Create the first one to start your pipeline.    |
| Submit                      | Create invoice                                                |
| Action pending.             | Waiting for a human to approve: post invoice INV-0042.        |

## Motion and accessibility

Animate transform and opacity only, and never autoplay decoration; reduced-motion settings are respected. The target is WCAG 2.2 level AA: visible focus, 24 px minimum targets, keyboard access for everything, and support for forced colours, 320 px reflow and 400 percent zoom.

## Source

The brand book is `docs/brand/BRAND.md` and the design system is the `@obop/ui` package in the repository.
