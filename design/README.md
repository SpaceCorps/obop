# Design assets

- `ascii-field.js`: the ambient "field of dim monospace characters" behind a hero (the look of the
  Claude Code desktop home). Decorative only: `aria-hidden` canvas, pauses when the tab is hidden,
  one still frame for `prefers-reduced-motion`, no content in the canvas. Colours come from the CSS
  custom properties `--ascii-ink` and `--ascii-accent` on the canvas, and a radial `mask-image` keeps
  the centre quiet so the headline reads. See `ascii-field.demo.html` for the markup and CSS.
