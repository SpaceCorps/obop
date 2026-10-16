//! The one small stylesheet (and the opt-in refresh script) served by this crate. The custom
//! properties use the names of `web/apps/backoffice/src/style.css` (`--accent`, `--panel`,
//! `--line`, `--fg`, `--muted`, `--radius`) so the sheet can be re-pointed at the shared design
//! tokens later: replace the `:root` blocks, nothing else refers to literal colours.

pub const CSS: &str = r#"
:root{
  color-scheme:light dark;
  --bg:#ffffff; --fg:#1a1d21; --muted:#4f5662; --panel:#f4f6f9; --line:#d3d8e0; --line-strong:#6f7785;
  --accent:#1a4fd6; --accent-fg:#ffffff; --accent-soft:#e6edff;
  --ok:#0b6b3a; --warn:#8a5a00; --bad:#b3261e; --ok-bg:#e3f4ea; --warn-bg:#fdf0d5; --bad-bg:#fde8e6;
  --radius:8px; --gap:1rem; --font:system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;
  --mono:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;
}
@media (prefers-color-scheme:dark){
  :root:not([data-theme="light"]){
    --bg:#0f1216; --fg:#e8eaed; --muted:#a9b1bd; --panel:#171b21; --line:#2e353f; --line-strong:#8a93a1;
    --accent:#8ab4ff; --accent-fg:#0b1220; --accent-soft:#1b2640;
    --ok:#62d392; --warn:#f0c15a; --bad:#ff8f86; --ok-bg:#12281d; --warn-bg:#2b2412; --bad-bg:#2e1715;
  }
}
:root[data-theme="dark"]{
  --bg:#0f1216; --fg:#e8eaed; --muted:#a9b1bd; --panel:#171b21; --line:#2e353f; --line-strong:#8a93a1;
  --accent:#8ab4ff; --accent-fg:#0b1220; --accent-soft:#1b2640;
  --ok:#62d392; --warn:#f0c15a; --bad:#ff8f86; --ok-bg:#12281d; --warn-bg:#2b2412; --bad-bg:#2e1715;
}
*,*::before,*::after{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--fg);font:1rem/1.6 var(--font)}
.wrap{max-width:62rem;margin:0 auto;padding:0 1rem}
a{color:var(--accent);text-underline-offset:.15em}
a:hover{text-decoration-thickness:2px}
:focus-visible{outline:3px solid var(--accent);outline-offset:2px;border-radius:2px}
.skip{position:absolute;left:-999px;top:0;background:var(--accent);color:var(--accent-fg);padding:.6rem 1rem;z-index:10}
.skip:focus{left:0}
.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
header.site{border-bottom:1px solid var(--line);background:var(--panel)}
header.site .wrap{display:flex;flex-wrap:wrap;gap:.5rem 1.25rem;align-items:center;padding-top:.6rem;padding-bottom:.6rem}
.brand{font-weight:700;font-size:1.25rem;color:var(--fg);text-decoration:none;margin-right:.5rem}
header.site ul,footer.site ul{list-style:none;margin:0;padding:0;display:flex;flex-wrap:wrap;gap:.25rem 1rem}
header.site nav a,footer.site a{display:inline-block;padding:.35rem 0;min-height:1.75rem}
header.site nav a[aria-current="page"]{font-weight:700;color:var(--fg)}
.search{display:flex;gap:.4rem;margin-left:auto}
.acct{display:flex;gap:.6rem;align-items:center;flex-wrap:wrap}
main{padding-top:1rem;padding-bottom:3rem;min-height:60vh}
main:focus{outline:none}
h1{font-size:1.9rem;line-height:1.25;margin:.5rem 0 .75rem}
h2{font-size:1.35rem;margin:2rem 0 .75rem}
h3{font-size:1.1rem}
.lede{color:var(--muted);margin-top:0}
.crumbs ol{list-style:none;display:flex;flex-wrap:wrap;gap:.25rem;margin:0;padding:0;color:var(--muted);font-size:.9rem}
.crumbs li+li::before{content:"/";margin-right:.25rem}
.crumbs a{color:var(--muted)}
input,textarea,select,button,.btn{font:inherit}
input[type=text],input[type=email],input[type=password],input[type=search],textarea,select{
  width:100%;padding:.55rem .7rem;background:var(--bg);color:var(--fg);border:1px solid var(--line-strong);border-radius:var(--radius);min-height:2.5rem}
.search input{width:14rem;max-width:100%}
textarea{min-height:11rem;font-family:var(--mono);font-size:.95rem}
label{display:block;font-weight:600;margin:1rem 0 .3rem}
.help{color:var(--muted);font-size:.9rem;margin:.25rem 0}
button,.btn{display:inline-block;background:var(--accent);color:var(--accent-fg);border:1px solid var(--accent);border-radius:var(--radius);padding:.5rem 1rem;min-height:2.5rem;cursor:pointer;text-decoration:none;font-weight:600}
button.secondary,.btn.secondary{background:transparent;color:var(--accent)}
button.link{background:none;border:0;color:var(--accent);padding:.35rem 0;min-height:0;text-decoration:underline;font-weight:400}
.hp{position:absolute;left:-9999px;width:1px;height:1px;overflow:hidden}
.panel,.card{background:var(--panel);border:1px solid var(--line);border-radius:var(--radius);padding:1rem}
.list{list-style:none;margin:0;padding:0;border:1px solid var(--line);border-radius:var(--radius)}
.list>li{padding:.85rem 1rem;border-top:1px solid var(--line)}
.list>li:first-child{border-top:0}
.list h2,.list h3{margin:0 0 .15rem;font-size:1.1rem}
.meta{color:var(--muted);font-size:.9rem;display:flex;flex-wrap:wrap;gap:.25rem 1rem}
.badge{display:inline-block;font-size:.78rem;font-weight:700;border:1px solid var(--line-strong);border-radius:99px;padding:0 .5rem;margin-left:.35rem;color:var(--fg);background:var(--bg)}
.badge.staff{border-color:var(--accent);color:var(--accent)}
.post{border:1px solid var(--line);border-radius:var(--radius);margin:1rem 0;background:var(--bg)}
.post>header{display:flex;flex-wrap:wrap;gap:.25rem 1rem;justify-content:space-between;padding:.6rem 1rem;background:var(--panel);border-bottom:1px solid var(--line);border-radius:var(--radius) var(--radius) 0 0}
.post .body{padding:0 1rem;overflow-wrap:anywhere}
.post .actions{padding:.25rem 1rem .75rem;display:flex;flex-wrap:wrap;gap:.5rem 1rem;align-items:center}
.post .removed{color:var(--muted);font-style:italic}
.body pre{background:var(--panel);border:1px solid var(--line);border-radius:var(--radius);padding:.75rem;overflow:auto;font:.9rem/1.5 var(--mono)}
.body code{font-family:var(--mono);font-size:.92em;background:var(--panel);padding:.1em .3em;border-radius:4px}
.body pre code{background:none;padding:0}
.body blockquote{margin:1rem 0;padding:.1rem 1rem;border-left:4px solid var(--line-strong);color:var(--muted)}
.body table{border-collapse:collapse}
.body th,.body td{border:1px solid var(--line-strong);padding:.3rem .6rem}
.pager ul{list-style:none;display:flex;gap:1rem;padding:0;margin:1.5rem 0;align-items:center;flex-wrap:wrap}
.alert{border:1px solid var(--bad);background:var(--bad-bg);color:var(--fg);padding:.75rem 1rem;border-radius:var(--radius);margin:1rem 0}
.notice{border:1px solid var(--line-strong);background:var(--panel);padding:.75rem 1rem;border-radius:var(--radius);margin:1rem 0}
.cols{display:grid;gap:1rem;grid-template-columns:repeat(auto-fit,minmax(16rem,1fr))}
table.data{border-collapse:collapse;width:100%}
table.data th,table.data td{text-align:left;border-top:1px solid var(--line);padding:.5rem .6rem;vertical-align:top}
.modbar{display:flex;flex-wrap:wrap;gap:.5rem;align-items:center;margin:.75rem 0}
.modbar form{display:inline-flex;gap:.4rem;align-items:center}
.modbar select{width:auto}
/* status page */
.banner{display:flex;align-items:center;gap:.75rem;padding:1rem 1.25rem;border-radius:var(--radius);border:2px solid var(--ok);background:var(--ok-bg);font-size:1.25rem;font-weight:700;margin:1rem 0}
.banner.minor{border-color:var(--warn);background:var(--warn-bg)}
.banner.major,.banner.critical{border-color:var(--bad);background:var(--bad-bg)}
.banner .ico{font-size:1.5rem;line-height:1}
.comp{border:1px solid var(--line);border-radius:var(--radius);padding:.85rem 1rem;margin:.75rem 0;background:var(--bg)}
.comp header{display:flex;flex-wrap:wrap;gap:.25rem 1rem;justify-content:space-between;align-items:baseline}
.comp h3{margin:0}
.state{font-weight:700}
.state.operational{color:var(--ok)}.state.degraded{color:var(--warn)}.state.partial,.state.major{color:var(--bad)}.state.maintenance{color:var(--accent)}.state.nodata{color:var(--muted)}
.bar{list-style:none;display:flex;gap:2px;margin:.6rem 0 .25rem;padding:0;height:2.1rem}
.bar li{flex:1 1 0;min-width:2px;border-radius:2px;background:var(--ok);border-top:3px solid var(--ok)}
.bar li.degraded{background:repeating-linear-gradient(135deg,var(--warn) 0 3px,var(--warn-bg) 3px 6px);border-top-color:var(--warn)}
.bar li.outage{background:repeating-linear-gradient(0deg,var(--bad) 0 2px,var(--bad-bg) 2px 5px);border-top:8px solid var(--bad)}
.bar li.nodata{background:var(--line);border-top-color:var(--line-strong);opacity:.6}
.barlabels{display:flex;justify-content:space-between;color:var(--muted);font-size:.82rem}
.uptimes{display:flex;flex-wrap:wrap;gap:.25rem 1.5rem;margin:.5rem 0 0;padding:0;list-style:none;color:var(--muted);font-size:.92rem}
.uptimes b{color:var(--fg)}
.timeline{list-style:none;padding:0;margin:.5rem 0}
.timeline li{border-left:3px solid var(--line-strong);padding:.1rem 0 .1rem .9rem;margin:.6rem 0}
.impact-minor{border-color:var(--warn)}.impact-major,.impact-critical{border-color:var(--bad)}
footer.site{border-top:1px solid var(--line);background:var(--panel);padding:1rem 0;color:var(--muted);font-size:.92rem}
footer.site .wrap{display:flex;flex-direction:column;gap:.5rem}
@media (max-width:40rem){h1{font-size:1.5rem}.search{margin-left:0;width:100%}.search input{flex:1}}
@media (prefers-reduced-motion:no-preference){a,button{transition:background-color .15s,color .15s}}
@media print{header.site,footer.site,.search,.actions,.modbar{display:none}}
"#;

/// Opt-in auto refresh for the status page, reached only by the explicit "turn on" link. It
/// reloads the page after a minute; nothing else on the site runs script.
pub const REFRESH_JS: &str = "setTimeout(function(){location.reload()},60000);\n";
