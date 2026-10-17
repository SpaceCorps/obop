//! The one stylesheet and the one script of the public site. The stylesheet is generated from
//! the instance settings (the accent) so it can stay a same-origin file under a strict CSP; its
//! URL carries a hash of its content and is served as immutable.

use crate::theme;

/// Tints for product cards: seven hues around the accent, light and dark.
const TINTS: usize = 7;

fn vars(accent: &str) -> String {
    let a = theme::resolve(accent);
    let mut css = String::new();
    css.push_str(&format!(
        ":root{{--accent:{};--fill:{};--on-fill:{};--ink:{}}}\n",
        a.chosen, a.light.fill, a.light.on_fill, a.light.ink
    ));
    for i in 0..TINTS {
        let (l, _) = theme::card_tints(&a.chosen, i as u64);
        css.push_str(&format!(".t{i}{{--tint:{l}}}\n"));
    }
    css.push_str(&format!(
        "@media (prefers-color-scheme:dark){{:root{{--fill:{};--on-fill:{};--ink:{}}}\n",
        a.dark.fill, a.dark.on_fill, a.dark.ink
    ));
    for i in 0..TINTS {
        let (_, d) = theme::card_tints(&a.chosen, i as u64);
        css.push_str(&format!(".t{i}{{--tint:{d}}}\n"));
    }
    css.push_str("}\n");
    css
}

/// The complete stylesheet for an accent colour.
pub fn css(accent: &str) -> String {
    format!("{}{BASE}", vars(accent))
}

const BASE: &str = r#"
:root{
  --bg:#f7f6f2;--panel:#fff;--fg:#1b1c20;--muted:#585c68;--line:#e2dfd6;--soft:#efede6;
  --danger:#b42318;--danger-bg:#fef3f2;--ok:#05603a;--ok-bg:#ecfdf3;
  --radius:14px;--shadow:0 1px 2px rgba(20,20,30,.05),0 6px 18px rgba(20,20,30,.06);
  --font:ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",Arial,"Noto Sans",sans-serif;
  --serif:ui-serif,"Iowan Old Style","Palatino Linotype",Palatino,Georgia,serif;
  color-scheme:light dark;
}
@media (prefers-color-scheme:dark){
  :root{--bg:#121317;--panel:#1b1d23;--fg:#ececf1;--muted:#a6aab6;--line:#2c2f38;--soft:#23252d;
    --danger:#ffb4ab;--danger-bg:#3a1613;--ok:#8fe0b4;--ok-bg:#0f2a1c;--shadow:none}
}
*,*::before,*::after{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.55 var(--font);min-height:100vh;display:flex;flex-direction:column}
img,svg{max-width:100%;height:auto}
a{color:var(--ink);text-underline-offset:.18em;text-decoration-thickness:1px}
a:hover{text-decoration-thickness:2px}
:focus-visible{outline:3px solid var(--ink);outline-offset:2px;border-radius:4px}
.wrap{width:100%;max-width:1120px;margin:0 auto;padding:0 20px}
.sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
.skip{position:absolute;left:12px;top:-60px;background:var(--fill);color:var(--on-fill);padding:10px 16px;border-radius:0 0 10px 10px;z-index:10;font-weight:600;text-decoration:none}
.skip:focus{top:0}
.hp{position:absolute;left:-9999px;top:auto;width:1px;height:1px;overflow:hidden}

/* header */
header.site{background:var(--panel);border-bottom:1px solid var(--line)}
header.site .bar{display:flex;align-items:center;gap:20px;min-height:68px;flex-wrap:wrap;padding-top:8px;padding-bottom:8px}
.brand{display:inline-flex;align-items:center;gap:10px;color:var(--fg);text-decoration:none;font-weight:700;font-size:1.15rem;letter-spacing:-.01em}
.brand img{height:34px;width:auto;display:block}
.brand:hover{text-decoration:none}
nav.main{flex:1}
nav.main ul{display:flex;gap:4px;list-style:none;margin:0;padding:0;flex-wrap:wrap}
nav.main a{display:inline-flex;align-items:center;min-height:44px;padding:0 12px;border-radius:10px;color:var(--fg);text-decoration:none;font-weight:500}
nav.main a:hover{background:var(--soft)}
nav.main a[aria-current="page"]{background:var(--soft);color:var(--ink);font-weight:600}
.cartlink{display:inline-flex;align-items:center;gap:8px;min-height:44px;padding:0 14px;border-radius:999px;border:1px solid var(--line);color:var(--fg);text-decoration:none;font-weight:600;background:var(--panel)}
.cartlink:hover{border-color:var(--ink)}
.cartlink .n{display:inline-block;min-width:22px;padding:1px 7px;border-radius:999px;background:var(--fill);color:var(--on-fill);font-size:.8rem;text-align:center}
.cartlink .n:empty{display:none}

main.wrap{flex:1;padding-top:32px;padding-bottom:56px}
h1{font-size:clamp(1.9rem,4vw,2.7rem);line-height:1.12;letter-spacing:-.02em;margin:0 0 .5em}
h2{font-size:1.5rem;line-height:1.2;letter-spacing:-.01em;margin:1.8em 0 .6em}
h3{font-size:1.2rem;margin:1.5em 0 .5em}
.lede{font-size:1.2rem;color:var(--muted);max-width:60ch;margin:0 0 1.2em}
.muted{color:var(--muted)}
.crumbs ol{display:flex;gap:6px;list-style:none;margin:0 0 18px;padding:0;flex-wrap:wrap;font-size:.92rem;color:var(--muted)}
.crumbs li+li::before{content:"/";margin-right:6px;color:var(--muted)}
.crumbs a{color:var(--muted)}

/* buttons and forms */
.btn,button.btn{display:inline-flex;align-items:center;justify-content:center;gap:8px;min-height:46px;padding:0 22px;border-radius:12px;border:1px solid var(--fill);background:var(--fill);color:var(--on-fill);font:600 1rem var(--font);text-decoration:none;cursor:pointer;transition:filter .15s}
.btn:hover{filter:brightness(1.07);text-decoration:none}
.btn.secondary{background:transparent;color:var(--ink);border-color:var(--line)}
.btn.secondary:hover{border-color:var(--ink);filter:none}
.btn.small{min-height:40px;padding:0 14px;font-size:.92rem}
.linkbtn{background:none;border:0;padding:0;font:inherit;color:var(--ink);text-decoration:underline;cursor:pointer;min-height:40px}
label{display:block;font-weight:600;margin:0 0 6px}
.field{margin:0 0 18px}
.field .help{display:block;font-weight:400;color:var(--muted);font-size:.9rem;margin-top:4px}
.req{color:var(--muted);font-weight:400;font-size:.85rem}
input[type=text],input[type=email],input[type=tel],input[type=search],input[type=number],textarea,select{width:100%;min-height:46px;padding:10px 14px;border:1px solid #8b8f9a;border-radius:10px;background:var(--panel);color:var(--fg);font:inherit}
textarea{min-height:130px;resize:vertical}
input[type=number]{max-width:96px}
input[type=checkbox],input[type=radio]{width:20px;height:20px;accent-color:var(--fill);margin:0}
.check{display:flex;gap:10px;align-items:flex-start;font-weight:400}
.check input{margin-top:2px;flex:none}
.err-sum{border:1px solid var(--danger);background:var(--danger-bg);color:var(--danger);border-radius:12px;padding:14px 18px;margin:0 0 22px}
.err-sum ul{margin:6px 0 0;padding-left:20px}
.notice{border:1px solid var(--line);background:var(--panel);border-radius:12px;padding:14px 18px;margin:0 0 22px}
.notice.ok{border-color:var(--ok);background:var(--ok-bg);color:var(--ok)}
.formcard{background:var(--panel);border:1px solid var(--line);border-radius:var(--radius);padding:24px;box-shadow:var(--shadow);max-width:640px}

/* home */
.hero{padding:40px 0 28px}
.hero h1{font-size:clamp(2.2rem,5.5vw,3.6rem);max-width:18ch}
.hero .actions{display:flex;gap:12px;flex-wrap:wrap;margin-top:8px}
.section{margin-top:44px}
.section>header{display:flex;align-items:baseline;justify-content:space-between;gap:16px;flex-wrap:wrap;margin-bottom:16px}
.section h2{margin:0}

/* product cards */
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:20px;list-style:none;margin:0;padding:0}
.card{position:relative;display:flex;flex-direction:column;background:var(--panel);border:1px solid var(--line);border-radius:var(--radius);overflow:hidden;transition:border-color .15s,transform .15s}
.card:hover{border-color:var(--ink)}
.art{aspect-ratio:4/3;display:flex;align-items:center;justify-content:center;background:var(--tint,var(--soft));color:var(--ink);font:700 3.4rem/1 var(--serif);letter-spacing:-.03em;user-select:none}
.art.media{background:var(--soft);overflow:hidden;font-size:0}
.art.media img{width:100%;height:100%;object-fit:cover;display:block}
.card .body{padding:14px 16px 18px;display:flex;flex-direction:column;gap:4px;flex:1}
.card h3{margin:0;font-size:1.05rem;line-height:1.3}
.card h3 a{color:var(--fg);text-decoration:none}
.card h3 a::after{content:"";position:absolute;inset:0}
.card .sum{color:var(--muted);font-size:.93rem;margin:0}
.price{font-weight:700;font-size:1.05rem;font-variant-numeric:tabular-nums}
.price .was{font-weight:400;color:var(--muted);text-decoration:line-through;margin-left:8px;font-size:.92rem}
.badge{display:inline-block;padding:2px 10px;border-radius:999px;font-size:.78rem;font-weight:700;background:var(--soft);color:var(--ink);border:1px solid var(--line)}
.badge.warn{background:var(--danger-bg);color:var(--danger);border-color:var(--danger)}
.cardtop{position:absolute;top:10px;left:10px}

/* shop layout */
.shoptop{display:flex;gap:16px;flex-wrap:wrap;align-items:end;justify-content:space-between;margin-bottom:20px}
.searchform{display:flex;gap:8px;align-items:end}
.searchform .field{margin:0}
.searchform input[type=search]{min-width:220px}
.chips{display:flex;flex-wrap:wrap;gap:8px;list-style:none;margin:0 0 22px;padding:0}
.chips a{display:inline-flex;align-items:center;min-height:40px;padding:0 16px;border-radius:999px;border:1px solid var(--line);background:var(--panel);color:var(--fg);text-decoration:none;font-weight:500}
.chips a:hover{border-color:var(--ink)}
.chips a[aria-current="true"]{background:var(--fill);color:var(--on-fill);border-color:var(--fill)}
.pager{display:flex;gap:12px;align-items:center;justify-content:center;margin-top:32px;flex-wrap:wrap}

/* product page */
.product{display:grid;gap:32px;grid-template-columns:1fr}
@media (min-width:820px){.product{grid-template-columns:minmax(0,1fr) minmax(0,1fr);align-items:start}}
.product .art{border-radius:var(--radius);border:1px solid var(--line);font-size:7rem}
.gallery{min-width:0}
.gallery .main{margin:0;border-radius:var(--radius);border:1px solid var(--line);overflow:hidden;background:var(--soft)}
.gallery .main img,.cover img{display:block;width:100%;height:auto}
.gallery .main a,.thumbs a{display:block}
.thumbs{display:grid;grid-template-columns:repeat(auto-fill,minmax(84px,1fr));gap:10px;list-style:none;margin:12px 0 0;padding:0}
.thumbs li{border:1px solid var(--line);border-radius:10px;overflow:hidden;background:var(--soft)}
.thumbs a:hover{opacity:.85}
.thumbs img{display:block;width:100%;height:auto;aspect-ratio:1/1;object-fit:cover}
.cover{margin:0 0 24px;border-radius:var(--radius);overflow:hidden;border:1px solid var(--line);background:var(--soft);max-width:1080px}
.product .buy{display:flex;gap:12px;align-items:end;flex-wrap:wrap;margin:20px 0}
.product .big{font-size:1.8rem;margin:0 0 12px}
.product h1{margin-bottom:.25em}
.product .buy .field{margin:0}
.formcard h2:first-child{margin-top:0}
.meta{color:var(--muted);font-size:.92rem;display:flex;gap:16px;flex-wrap:wrap;align-items:center}

/* cart */
.lines{list-style:none;margin:0 0 24px;padding:0;border:1px solid var(--line);border-radius:var(--radius);background:var(--panel);overflow:hidden}
.line{display:grid;grid-template-columns:1fr auto;gap:6px 18px;padding:16px 18px;border-top:1px solid var(--line);align-items:center}
.line:first-child{border-top:0}
.line .name{font-weight:600}
.line .each{color:var(--muted);font-size:.92rem}
.line .qty{display:flex;gap:8px;align-items:center}
.line .qty input{max-width:78px}
.line .sum{font-weight:700;text-align:right;font-variant-numeric:tabular-nums}
.totals{margin:0 0 24px;display:grid;grid-template-columns:1fr auto;gap:6px 24px;max-width:420px;margin-left:auto}
.totals dt{color:var(--muted)}
.totals dd{margin:0;text-align:right;font-variant-numeric:tabular-nums}
.totals .grand{font-weight:700;font-size:1.2rem;color:var(--fg)}
.cartgrid{display:grid;gap:32px;grid-template-columns:1fr}
@media (min-width:900px){.cartgrid{grid-template-columns:minmax(0,1.2fr) minmax(0,1fr);align-items:start}}
fieldset{border:0;padding:0;margin:0 0 18px}
legend{font-weight:600;padding:0;margin:0 0 8px}
.opt{display:flex;gap:12px;align-items:center;padding:12px 14px;border:1px solid var(--line);border-radius:12px;margin:0 0 8px;background:var(--panel);font-weight:400;cursor:pointer}
.opt:has(input:checked){border-color:var(--ink)}
.opt .grow{flex:1}
details.more{margin:0 0 24px}
details.more summary{cursor:pointer;min-height:44px;display:flex;align-items:center;font-weight:600;color:var(--ink)}

/* order confirmation */
dl.facts{display:grid;grid-template-columns:max-content 1fr;gap:8px 22px;margin:0 0 24px}
dl.facts dt{color:var(--muted)}
dl.facts dd{margin:0;white-space:pre-line}

/* rich text */
.prose{max-width:68ch;font-size:1.06rem;line-height:1.75}
.prose p{margin:0 0 1.1em}
.prose h2,.prose h3,.prose h4{margin-top:1.8em}
.prose ul,.prose ol{padding-left:1.4em;margin:0 0 1.1em}
.prose li{margin:.3em 0}
.prose blockquote{margin:1.4em 0;padding:.2em 1.2em;border-left:4px solid var(--fill);color:var(--muted);font-family:var(--serif);font-size:1.15em}
.prose code{font:.9em ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:var(--soft);padding:.15em .4em;border-radius:6px}
.prose pre{background:var(--soft);padding:16px;border-radius:12px;overflow:auto}
.prose pre code{background:none;padding:0}
.prose table{border-collapse:collapse;width:100%;margin:0 0 1.2em}
.prose th,.prose td{border:1px solid var(--line);padding:8px 12px;text-align:left}
.prose hr{border:0;border-top:1px solid var(--line);margin:2em 0}

/* blog */
.posts{list-style:none;margin:0;padding:0;display:grid;gap:18px}
.post{background:var(--panel);border:1px solid var(--line);border-radius:var(--radius);padding:22px 24px;position:relative}
.post:hover{border-color:var(--ink)}
.post h2,.post h3{margin:0 0 6px;font-size:1.35rem}
.post h2 a,.post h3 a{color:var(--fg);text-decoration:none}
.post h2 a::after,.post h3 a::after{content:"";position:absolute;inset:0}
.post .meta{margin-bottom:8px}
.post.withimg{display:grid;gap:16px 22px;grid-template-columns:1fr;padding:0;overflow:hidden}
@media (min-width:700px){.post.withimg{grid-template-columns:240px minmax(0,1fr);align-items:stretch}}
.post.withimg .postbody{padding:20px 24px 22px 0}
@media (max-width:699px){.post.withimg .postbody{padding:0 20px 20px}}
.postimg{background:var(--soft);min-height:100%}
.postimg img{display:block;width:100%;height:100%;object-fit:cover;aspect-ratio:3/2}
.post p{margin:0;color:var(--muted)}
.tags{display:flex;gap:8px;flex-wrap:wrap;list-style:none;margin:12px 0 0;padding:0}

/* footer */
footer.site{background:var(--panel);border-top:1px solid var(--line);padding:32px 0 40px;color:var(--muted);font-size:.94rem}
footer.site .cols{display:flex;gap:28px 48px;flex-wrap:wrap;justify-content:space-between}
footer.site ul{list-style:none;margin:0;padding:0;display:flex;gap:6px 16px;flex-wrap:wrap}
footer.site a{color:var(--muted)}
footer.site a:hover,footer.site a[aria-current="true"]{color:var(--ink)}
footer.site a{display:inline-flex;align-items:center;min-height:32px}
footer.site h2{font-size:.8rem;text-transform:uppercase;letter-spacing:.08em;margin:0 0 8px;color:var(--fg)}
footer.site p{margin:0 0 8px}

@media (max-width:600px){
  main.wrap{padding-top:22px}
  .grid{grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}
  .art{font-size:2.4rem;aspect-ratio:1/1}
  .card .body{padding:10px 12px 14px}
  .card h3{font-size:.98rem}
  .card .sum{display:none}
  .hero{padding-top:16px}
  .line{grid-template-columns:1fr}
  .line .sum{text-align:left}
  .totals{max-width:none}
  .searchform{width:100%}
  .searchform .field{flex:1}
  .searchform input[type=search]{min-width:0}
  nav.main{order:3;flex-basis:100%}
}
@media (prefers-reduced-motion:reduce){*{transition:none!important}}
@media print{header.site nav,footer.site,.btn,.cartlink{display:none}body{background:#fff;color:#000}}
"#;

/// The progressive-enhancement script: the cart in `localStorage`, the count in the header and a
/// live total at checkout. Without it everything still works through plain forms and links.
pub const JS: &str = r#"(function(){
"use strict";
var KEY="obop-cart-v1",MAX=50;
function load(){try{var v=JSON.parse(localStorage.getItem(KEY)||"[]");if(!Array.isArray(v))return[];return v.filter(function(x){return Array.isArray(x)&&typeof x[0]==="string"&&/^[A-Za-z0-9-]{8,64}$/.test(x[0])&&x[1]>=1&&x[1]<=99}).slice(0,MAX)}catch(e){return[]}}
function save(c){try{localStorage.setItem(KEY,JSON.stringify(c))}catch(e){}}
function count(c){return c.reduce(function(n,x){return n+x[1]},0)}
function paint(){var c=load(),n=count(c);document.querySelectorAll("[data-cart-count]").forEach(function(el){el.textContent=n?String(n):""});}
function add(id,q){var c=load(),f=c.filter(function(x){return x[0]===id})[0];if(f){f[1]=Math.min(99,f[1]+q)}else if(c.length<MAX){c.push([id,Math.min(99,q)])}save(c);paint()}
function qs(c){return c.map(function(x){return x[0]+"."+x[1]}).join("~")}
document.addEventListener("submit",function(e){
  var f=e.target;
  if(f.matches&&f.matches("form[data-add]")){
    e.preventDefault();
    var q=parseInt((f.querySelector("[name=addqty]")||{}).value||"1",10);if(!(q>=1&&q<=99))q=1;
    add(f.getAttribute("data-add"),q);
    var s=f.querySelector("[role=status]");if(s){s.textContent=f.getAttribute("data-added")||"OK"}
  }
});
var page=document.body;
if(page.hasAttribute("data-clear-cart")){save([]);}
if(page.hasAttribute("data-cart-page")){
  var items=page.getAttribute("data-items")||"";
  var c=load();
  if(!items&&c.length){location.replace("/cart?items="+encodeURIComponent(qs(c))+(page.getAttribute("data-lang")||""));return}
  if(items){
    var parsed=items.split("~").map(function(p){var a=p.split(".");return[a[0],parseInt(a[1],10)||1]}).filter(function(x){return x[0]});
    save(parsed);
  }
  var shipping=document.querySelectorAll("input[name=shipping_method_id]");
  var box=document.querySelector("[data-sub]");
  function money(minor,cur,lang){
    try{var f=new Intl.NumberFormat(lang,{style:"currency",currency:cur});var d=f.resolvedOptions().maximumFractionDigits;return f.format(minor/Math.pow(10,d))}catch(e){return String(minor)}
  }
  function recalc(){
    if(!box)return;
    var sub=parseInt(box.getAttribute("data-sub"),10)||0,cur=box.getAttribute("data-currency"),lang=box.getAttribute("data-lang")||"en",ship=0,any=false;
    shipping.forEach(function(r){if(r.checked){any=true;var p=parseInt(r.getAttribute("data-price"),10)||0,fo=r.getAttribute("data-free-over");ship=(fo&&sub>=parseInt(fo,10))?0:p}});
    var so=document.querySelector("[data-ship-out]"),to=document.querySelector("[data-total-out]");
    if(so&&any)so.textContent=ship?money(ship,cur,lang):so.getAttribute("data-free");
    if(to&&any)to.textContent=money(sub+ship,cur,lang);
  }
  shipping.forEach(function(r){r.addEventListener("change",recalc)});
  recalc();
}
paint();
})();
"#;
