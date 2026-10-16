// Ambient field of dim monospace characters behind a hero, like the Claude Code desktop home.
// Decorative only: the canvas is aria-hidden, holds no content, pauses when hidden, and renders
// a single still frame for people who prefer reduced motion.
const GLYPHS = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef{}[]<>/\\|=+-_*#$%:;.,";

function hash(x, y, t) {
  let h = (x * 374761393 + y * 668265263 + t * 2147483647) | 0;
  h = (h ^ (h >>> 13)) * 1274126177;
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

// Smooth value noise on a coarse lattice, drifting slowly with time.
function noise(x, y, t) {
  const xi = Math.floor(x), yi = Math.floor(y);
  const xf = x - xi, yf = y - yi;
  const u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf);
  const a = hash(xi, yi, 0), b = hash(xi + 1, yi, 0), c = hash(xi, yi + 1, 0), d = hash(xi + 1, yi + 1, 0);
  const n = a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
  return n * 0.75 + 0.25 * Math.sin(t + x * 1.7 + y * 1.3) * 0.5 + 0.125;
}

export function asciiField(canvas, options = {}) {
  const opt = { size: 12, fps: 10, ...options };
  const ctx = canvas.getContext("2d");
  const reduce = matchMedia("(prefers-reduced-motion: reduce)");
  let w = 0, h = 0, cols = 0, rows = 0, cw = 0, ch = 0, cells = [], raf = 0, last = 0, running = false;
  let mx = -1e4, my = -1e4;

  function color() {
    const cs = getComputedStyle(canvas);
    return { fg: cs.getPropertyValue("--ascii-ink").trim() || cs.color, accent: cs.getPropertyValue("--ascii-accent").trim() };
  }

  function resize() {
    const dpr = Math.min(devicePixelRatio || 1, 2);
    const r = canvas.getBoundingClientRect();
    w = Math.max(1, Math.floor(r.width)); h = Math.max(1, Math.floor(r.height));
    canvas.width = Math.floor(w * dpr); canvas.height = Math.floor(h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.font = `${opt.size}px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`;
    ctx.textBaseline = "top";
    cw = Math.ceil(ctx.measureText("0").width) + 2; ch = Math.round(opt.size * 1.45);
    cols = Math.ceil(w / cw); rows = Math.ceil(h / ch);
    cells = Array.from({ length: cols * rows }, (_, i) => GLYPHS[Math.floor(hash(i, 7, 3) * GLYPHS.length)]);
    draw(performance.now());
  }

  function draw(now) {
    const t = now / 4000;
    const { fg, accent } = color();
    ctx.clearRect(0, 0, w, h);
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        const i = y * cols + x;
        // Clusters: coarse noise decides where text lives; edges of the page are denser,
        // and the centre column stays quiet so the headline reads cleanly.
        const nx = x / 9, ny = y / 4.5;
        let d = noise(nx, ny, t) * 1.5 - 0.72;
        const cx = Math.abs(x / cols - 0.5) * 2; // 0 centre, 1 edge
        d *= 0.35 + cx * 0.9;
        if (d <= 0.04) continue;
        // Characters shuffle where the field is lively.
        if (hash(x, y, Math.floor(now / 380)) < 0.04 * d) cells[i] = GLYPHS[Math.floor(hash(x, y, Math.floor(now / 91)) * GLYPHS.length)];
        const dist = Math.hypot(x * cw - mx, y * ch - my);
        const near = Math.max(0, 1 - dist / 140);
        const a = Math.min(0.55, d * 0.5 + near * 0.35);
        ctx.fillStyle = near > 0.15 && accent ? accent : fg;
        ctx.globalAlpha = a;
        ctx.fillText(cells[i], x * cw, y * ch);
      }
    }
    ctx.globalAlpha = 1;
  }

  function loop(now) {
    if (!running) return;
    if (now - last >= 1000 / opt.fps) { last = now; draw(now); }
    raf = requestAnimationFrame(loop);
  }
  const start = () => { if (!running && !reduce.matches && !document.hidden) { running = true; raf = requestAnimationFrame(loop); } };
  const stop = () => { running = false; cancelAnimationFrame(raf); };

  const ro = new ResizeObserver(resize); ro.observe(canvas);
  const onVis = () => (document.hidden ? stop() : start());
  const onMove = (e) => { const r = canvas.getBoundingClientRect(); mx = e.clientX - r.left; my = e.clientY - r.top; };
  const onLeave = () => { mx = my = -1e4; };
  document.addEventListener("visibilitychange", onVis);
  addEventListener("pointermove", onMove, { passive: true });
  addEventListener("pointerleave", onLeave);
  reduce.addEventListener("change", () => (reduce.matches ? (stop(), draw(performance.now())) : start()));
  resize(); start();
  return () => { stop(); ro.disconnect(); document.removeEventListener("visibilitychange", onVis); removeEventListener("pointermove", onMove); removeEventListener("pointerleave", onLeave); };
}
