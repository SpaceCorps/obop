# Design system: `@obop/ui`

The package `web/packages/ui` is the one design system behind the back office, the platform app and the
landing page. The rules live in [BRAND.md](BRAND.md); this page is how the code is organised.

## Contents

| Path | What |
|---|---|
| `src/tokens.css` | Design tokens as CSS custom properties: OKLCH ramps, semantic colours, type, spacing, radius, elevation, motion, density. Fonts (Geist, Geist Mono) are imported from fontsource and self-hosted. |
| `src/base.css` | Reset, document defaults, headings, focus ring, utilities (`.sr-only`, `.hint`, `.tabular`). Logical properties only. |
| `src/components.css` | Styles of every primitive. No hard-coded colours, sizes or radii. |
| `src/styles.css` | Imports the three above; apps import `@obop/ui/styles.css`. |
| `src/colors.ts` | Pure colour maths: hex and OKLCH, WCAG contrast, ramps, accent resolution and the AA correction. |
| `src/theme.ts` | `applyTheme()`, `ACCENTS`, `DEFAULT_THEME`. |
| `src/icon-data.ts`, `icons.tsx` | The icon set (shape data) and `Icon`, `AppIcon`. |
| `src/app-colors.ts` | Module id to glyph and tile colour. |
| `src/logo-data.ts`, `logo.tsx` | Logo geometry and the `Logo` component. |
| `src/primitives.tsx` | Button, IconButton, Input, Select, Textarea, Checkbox, Switch, Badge, Chip, Kbd, Avatar, Spinner, Notice, Panel, Card, Table, Skeleton, EmptyState, Tabs, Segmented, Popover, Menu, Tooltip, Dialog (and sheet), Toast. |
| `src/hooks.ts` | `useDismiss`, `useFocusTrap`, `rovingKeys`. |
| `src/static.ts` | The logo, icons and app tiles as strings, for non-React pages. |

## Theming

The root element carries `data-theme` (`light`, `dark`; absent follows the system), `data-density`
(`comfortable`, `compact`) and `data-radius` (`sharp`, `soft`, `round`). `applyTheme()` sets those and
the accent's custom properties (`--accent`, `--accent-50..950`, `--accent-fill-l/-d`, `--accent-ink-l/-d`,
`--on-accent-l/-d`, `--accent-h`, `--radius`) inline. tokens.css then selects light or dark values, so a
switch is an attribute change, never a second bundle. The dark theme appears twice in tokens.css (once for
`data-theme="dark"`, once for the system preference); a test keeps the copies identical.

## Using it

```tsx
import "@obop/ui/styles.css";
import { Button, Input, Icon, AppIcon, Logo } from "@obop/ui";
```

The landing page is static: it imports `@obop/ui/styles.css` from CSS and writes `<ui-logo>`, `<ui-icon>`
and `<ui-app>` placeholders that a Vite plugin (`web/apps/landing/vite.config.ts`) replaces with the same
inline SVG using `@obop/ui/static`.

## Adding a component

1. Write it in `src/primitives.tsx` (or a new file exported from `src/index.ts`): native element first,
   roles and keyboard second, a required accessible name where the visual has none.
2. Style it in `src/components.css` using tokens only. Use logical properties (`margin-inline`, `inset-block`).
3. If it has hover or focus styling worth showing, add a `[data-state="hover|focus"]` twin of the selector.
4. Add a `Specimen` (states and a props table) in `web/apps/backoffice/src/brand/Components.tsx`, and a
   rule in BRAND.md if it adds one.
5. Run `vp check`, `vp test` and `vp build` in `web/packages/ui` and in the apps.

## Adding an icon

Add the shapes to `UI_ICONS` (or `APP_ICONS` plus a colour in `app-colors.ts`) in `icon-data.ts` on the
24 px grid with a 1.75 px stroke. It appears in the Brand book automatically.

## How the Brand book stays in sync

The Brand book (`web/apps/backoffice/src/brand/`) renders from the package, not from copies: ramps and
semantic tokens are read from the live document (the contrast numbers are measured in the browser), the
icon grid iterates `UI_ICONS` and `APP_ICONS`, the components are the real primitives, and the patterns
run the real control panel, filter panel, list and kanban on sample data in a private data layer. What
cannot be derived (rules, prose, props tables) is written next to the specimen; changing a component
means updating its specimen in the same change. `tokens.test.ts` fails when tokens.css and colors.ts drift.
