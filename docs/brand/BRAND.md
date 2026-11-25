# obop brand book

This document is the source of truth for how obop looks and sounds. The living demo is the **Brand
book** in the product (Customize > Brand book, `#/customize/brand`), rendered with the real
components in the current theme. Implementation: `web/packages/ui` (see [DESIGN-SYSTEM.md](DESIGN-SYSTEM.md)).
Accessibility target: WCAG 2.2 level AA ([ux.md](../spec/ux.md) 4.9 and 4.10).

Personality: calm, precise, confident. One business, one platform; privacy-respecting; agent-ready.

## 1. Name and logo

- The name is **obop**: lowercase always, including at the start of a sentence. Never "Obop", "OBOP"
  or "the obop".
- The logo is a mark and a wordmark, both inline SVG in one colour (`currentColor`).
  - **Mark:** a ring open on one side with a dot in the opening: one platform, the business in orbit.
    Geometry: 32 x 32, ring radius 11, stroke 4, dot radius 4.2.
  - **Wordmark:** lowercase, geometric, monoline (stroke 3.4), drawn from the same circles, so it needs
    no font.
  - **Variants:** full (mark + wordmark), mark alone (favicon, app badge), wordmark alone.
- **Colour:** ink on light surfaces, near-white on dark, the on-accent colour on an accent
  background, or pure black for one-colour print. Never gradients, outlines or shadows.
- **Clear space:** x on every side, where x is half the height of the logo.
- **Minimum size:** mark 16 px; full logo 20 px high.
- **Misuse:** do not stretch, rotate, recolour with effects, or place on low-contrast or busy backgrounds.
- **Favicon:** the mark in white on an accent tile (`favicon.svg` in each app's `public/`).

## 2. Colour

All colours are OKLCH ramps with the same lightness ladder in every hue, 50 to 950:

| Step | 50 | 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900 | 950 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| L | .972 | .945 | .89 | .825 | .745 | .66 | .57 | .485 | .40 | .32 | .235 |

- **Neutral:** cool grey (hue 265, chroma 0.004 to 0.022).
- **Accent:** one hue chosen by the person or the instance admin; its ramp is generated.
- **Signals:** success (152), warning (78), danger (22), info (245).
- **Semantic tokens** are what components use: `--bg`, `--surface`, `--surface-2/3`, `--border`,
  `--border-strong`, `--border-control`, `--text`, `--text-muted`, `--text-subtle`, `--accent-fill`,
  `--accent-ink`, `--on-accent`, `--success/warning/danger/info` with `-ink` and `-soft`, `--chart-1..8`.
  Components never use ramp steps directly.
- **Automatic AA check.** The theme picker measures the chosen accent in light and dark: text on the
  accent fill 4.5:1, fill against panel and canvas 3:1, accent text on panel, canvas and a selected row
  4.5:1. A colour that fails is moved along lightness until it passes (the brand colour on app tiles is
  kept); the ratios and the correction are shown. Semantic text and control-border tokens are checked by
  unit tests (`tokens.test.ts`).
- Never rely on colour alone: pair it with text, an icon or a position.

## 3. Typography

- **Sans: Geist Variable** (SIL OFL 1.1, `@fontsource-variable/geist`, self-hosted). Precise and calm at
  small sizes, true tabular lining figures for money, one variable file per script, Latin, Latin
  Extended, Cyrillic and Vietnamese. Chosen over Inter for a less generic voice at the same legibility.
- **Mono: Geist Mono Variable** (OFL 1.1) for code, identifiers, API paths.
- No fonts from third-party CDNs; fallback stack `ui-sans-serif, system-ui, ...`.
- **Scale** (rem on a 16 px root): display 72, 5xl 52, 4xl 38, 3xl 30, 2xl 24, xl 20, lg 18, md 16,
  sm 13, xs 12, 2xs 11. Body is 15 px (comfortable) or 13 px (compact). Line heights 1.15 (headings),
  1.3, 1.5 (body), 1.65 (reading). Weights 400, 500, 600, 700; headings 600 with tracking
  -0.022em; overlines uppercase +0.06em.
- **Numbers are tabular** (`font-variant-numeric: tabular-nums lining-nums`) everywhere they line up.

## 4. Space, radius, elevation

- **Spacing:** 4 px base: 4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 80, 96 (`--space-1..24`).
- **Radius:** one base value (Sharp 4, Soft 12, Round 20) and derived `xs, sm, md, lg, xl, pill`.
  Controls use `md`, panels `lg`, dialogs `xl`, badges and switches `pill`.
- **Elevation:** four cool-tinted shadows: 1 cards, panels and buttons; 2 hover lift; 3 menus, popovers,
  toasts; 4 dialogs and sheets. Dark mode deepens the shadows and adds an edge highlight.

## 5. Motion

- Durations: 90 ms (hover), 160 ms (menus, toggles), 240 ms (dialogs, panels), 380 ms (page level).
- Easing: `ease-out cubic-bezier(.16,1,.3,1)` for things appearing; `ease-in-out` for things moving
  across; `ease-in` for things leaving.
- Animate transform and opacity only. No autoplay, parallax or decorative loops.
- **Reduced motion:** all durations collapse to near zero, shimmer and spin stop (a static ring
  remains); state still changes immediately.

## 6. Iconography

- Hand-drawn set, 24 px grid, **1.75 px stroke**, round caps and joins, no fills except small dots.
- Icons inherit `currentColor`, are decorative (`aria-hidden`) unless given a label, and icon-only
  buttons require a label (`IconButton`).
- Every app has a glyph: CRM, Sales, Invoicing, Products, Contacts, Project, Helpdesk, Fleet,
  Inventory, Purchase, Manufacturing, HR, Timesheets, Planning, Field Service, Point of Sale,
  eCommerce, Website, Email Marketing, Documents, Sign, Subscriptions, Dashboards, Accounting,
  Knowledge, Appointments, Expenses, Customize; plus a generic app glyph and about 60 interface icons.
- **App tile:** rounded square (corner 28%), gradient from the app colour, white glyph. Tile colours
  sit at OKLCH lightness 0.52 to 0.60 so the glyph keeps at least 3:1. Admins can recolour; unknown
  modules get a stable colour from their id.

## 7. Components and patterns

Button (primary, secondary, ghost, danger; sizes; loading), IconButton, Input, Select, Textarea,
Checkbox, Switch, Badge, Chip, Card, Panel, Tabs, Segmented, Menu, Popover, Dialog, Sheet, Toast,
Tooltip, Table, Skeleton, EmptyState, Kbd, Avatar, Spinner, Notice. Rules:

- One primary action per view; labels are verbs.
- A visible label on every field; errors say what to do and sit under the field.
- Panels are the one framed surface; records open beside the list.
- Density: comfortable (36 px controls), compact (30 px, never below 24 px targets), touch (44 to 48 px).
- Module facts (D55) are badges with a word and, where colour carries a meaning, an icon: licence (neutral; warning for a proprietary or custom licence), maturity (stable success, beta info, experimental warning), core compatibility (success "Works with core 0.2"; danger with both versions when it does not), source (neutral). Red is kept for what cannot be installed. A dependency chip is satisfied, built in, will be installed, will be upgraded, missing or conflict. Dependencies are drawn as nested lists with connecting lines, never as colour alone. Specimens: Brand book, Components.

## 8. Voice and tone

Plain, specific, calm, honest, helpful, human.

| Don't | Do |
|---|---|
| Invalid input. | Enter an email like name@company.com. |
| Oops! Something went wrong! | Could not save the lead. Check your connection and try again. |
| No data. | No leads yet. Create the first one to start your pipeline. |
| Are you sure? | Delete 3 leads? This cannot be undone. |
| Submit | Create invoice |
| Operation completed successfully. | Invoice INV-0042 posted. |
| Action pending. | Waiting for a human to approve: post invoice INV-0042. |

Words: record (not entity), app (not module, in the UI), sign in, delete (irreversible) vs archive,
agent, approve. Sentence case everywhere; no exclamation marks; no invented customers, quotes or
metrics anywhere, including the landing page.

## 9. Accessibility

- Contrast: text 4.5:1, large text and UI 3:1; input outlines use `--border-control` (3:1).
- Targets 24 x 24 px minimum (WCAG 2.5.8), 36 px comfortable, 44 to 48 px touch.
- Focus: 2 px accent outline, 2 px offset, never removed; sticky bars never cover focus.
- Keyboard: Ctrl/Cmd+K palette, Esc closes, arrows in tabs, menus, lists and kanban, Shift+arrows move a
  card, Ctrl/Cmd+S saves; every drag has a non-drag twin (2.5.7).
- Live regions: toasts are polite `role="status"`; only errors use `role="alert"`.
- Forced colours, 320 px reflow, 400% zoom and reduced motion are supported.
