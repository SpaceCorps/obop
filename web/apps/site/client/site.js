// Progressive enhancement for the static site. Everything here is optional: every page reads and
// navigates without it. Plain script, no dependencies, no cookies (localStorage only).
(function () {
  "use strict";
  var doc = document;
  var root = doc.documentElement;

  function storage(op, key, value) {
    try {
      if (op === "get") return localStorage.getItem(key);
      if (op === "set") localStorage.setItem(key, value);
    } catch (e) {
      /* storage can be blocked; the page works without it */
    }
    return null;
  }

  // ---- Theme toggle ----
  var toggle = doc.querySelector(".theme-toggle");
  if (toggle) {
    toggle.hidden = false;
    toggle.addEventListener("click", function () {
      var current =
        root.getAttribute("data-theme") ||
        (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
      var next = current === "dark" ? "light" : "dark";
      root.setAttribute("data-theme", next);
      storage("set", "obop-theme", next);
    });
  }

  // ---- Strings for the page language (copy buttons, language hint) ----
  var hints = {};
  var hintEl = doc.getElementById("lang-hints");
  if (hintEl) {
    try {
      hints = JSON.parse(hintEl.textContent || "{}");
    } catch (e) {
      hints = {};
    }
  }
  var banner = doc.querySelector(".lang-banner");
  var pageLang = banner ? banner.getAttribute("data-page-lang") : "en";
  var strings = hints[pageLang] || {};

  // ---- Copy buttons on code blocks ----
  doc.querySelectorAll('div[class*="language-"] > button.copy').forEach(function (btn) {
    btn.setAttribute("aria-label", strings.copy || "Copy");
    btn.addEventListener("click", function () {
      var code = btn.parentElement && btn.parentElement.querySelector("pre code");
      if (!code || !navigator.clipboard) return;
      navigator.clipboard.writeText(code.innerText).then(function () {
        btn.classList.add("copied");
        btn.setAttribute("aria-label", strings.copied || "Copied");
        setTimeout(function () {
          btn.classList.remove("copied");
          btn.setAttribute("aria-label", strings.copy || "Copy");
        }, 1800);
      });
    });
  });

  // ---- Language hint: a dismissible bubble, never a redirect ----
  if (banner && storage("get", "obop-lang-dismissed") !== "1") {
    var prefs = navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language];
    var target = null;
    for (var i = 0; i < prefs.length; i++) {
      var base = String(prefs[i] || "").toLowerCase().split("-")[0];
      if (base === pageLang) break;
      if (hints[base]) {
        target = base;
        break;
      }
    }
    if (target) {
      var alt = doc.querySelector('link[rel="alternate"][hreflang="' + target + '"]');
      var url = alt ? new URL(alt.getAttribute("href"), location.href).pathname : target === "en" ? "/" : "/" + target + "/";
      banner.querySelector(".lang-text").textContent = hints[target].text;
      var go = banner.querySelector(".lang-go");
      go.textContent = hints[target].go;
      go.setAttribute("href", url);
      go.setAttribute("hreflang", target);
      go.setAttribute("lang", target);
      var dismiss = banner.querySelector(".lang-dismiss");
      dismiss.textContent = hints[target].dismiss;
      dismiss.setAttribute("lang", target);
      banner.querySelector(".lang-text").setAttribute("lang", target);
      banner.hidden = false;
      var close = function () {
        banner.hidden = true;
        storage("set", "obop-lang-dismissed", "1");
      };
      dismiss.addEventListener("click", close);
      go.addEventListener("click", function () {
        storage("set", "obop-lang-dismissed", "1");
      });
    }
  }

  // ---- Search ----
  var forms = doc.querySelectorAll("form.search");
  if (!forms.length) return;
  var indexes = {};

  function loadIndex(locale) {
    if (!indexes[locale]) {
      indexes[locale] = fetch("/search/" + locale + ".json")
        .then(function (r) {
          return r.json();
        })
        .catch(function () {
          return [];
        });
    }
    return indexes[locale];
  }

  function find(index, query, limit) {
    var terms = query.toLowerCase().split(/[^\p{L}\p{N}_.-]+/u).filter(Boolean);
    if (!terms.length) return [];
    var hits = [];
    index.forEach(function (d) {
      var title = d[1].toLowerCase();
      var heads = d[3].toLowerCase();
      var text = d[2].toLowerCase();
      var score = 0;
      for (var i = 0; i < terms.length; i++) {
        var t = terms[i];
        var s = 0;
        if (title.indexOf(t) >= 0) s += 8;
        if (heads.indexOf(t) >= 0) s += 4;
        if (text.indexOf(t) >= 0) s += 1;
        if (!s) return;
        score += s;
      }
      hits.push({ d: d, score: score });
    });
    hits.sort(function (a, b) {
      return b.score - a.score;
    });
    return hits.slice(0, limit).map(function (h) {
      return h.d;
    });
  }

  function snippet(text, query) {
    var term = query.toLowerCase().split(/\s+/)[0] || "";
    var at = term ? text.toLowerCase().indexOf(term) : -1;
    var start = Math.max(0, at - 40);
    return (start > 0 ? "..." : "") + text.slice(start, start + 140) + "...";
  }

  function render(list, results, query, rich, empty) {
    results.textContent = "";
    if (!query) {
      results.hidden = true;
      return;
    }
    if (!list.length) {
      var li0 = doc.createElement("li");
      li0.className = "none";
      li0.textContent = empty;
      results.appendChild(li0);
    }
    list.forEach(function (d) {
      var li = doc.createElement("li");
      var a = doc.createElement("a");
      a.href = d[0];
      var strong = doc.createElement("strong");
      strong.textContent = d[1];
      a.appendChild(strong);
      if (rich) {
        var p = doc.createElement("span");
        p.textContent = snippet(d[2], query);
        a.appendChild(p);
      }
      li.appendChild(a);
      results.appendChild(li);
    });
    results.hidden = false;
  }

  forms.forEach(function (form) {
    var input = form.querySelector("input[name=q]");
    var results = form.querySelector(".search-results");
    var locale = form.getAttribute("data-locale") || "en";
    var empty = form.getAttribute("data-empty") || "No results.";
    if (!input || !results) return;
    input.addEventListener("focus", function () {
      loadIndex(locale);
    });
    input.addEventListener("input", function () {
      var q = input.value.trim();
      loadIndex(locale).then(function (idx) {
        render(find(idx, q, 7), results, q, false, empty);
      });
    });
    doc.addEventListener("click", function (e) {
      if (!form.contains(e.target)) results.hidden = true;
    });
    input.addEventListener("keydown", function (e) {
      if (e.key === "Escape") results.hidden = true;
    });
  });

  // The search page: results for ?q= with excerpts.
  var page = doc.getElementById("search-page-results");
  if (page) {
    var loc = page.getAttribute("data-locale") || "en";
    var params = new URLSearchParams(location.search);
    var big = doc.getElementById("q-page");
    var list = doc.createElement("ul");
    list.className = "search-list";
    page.appendChild(list);
    var run = function (q) {
      loadIndex(loc).then(function (idx) {
        render(find(idx, q, 30), list, q, true, page.getAttribute("data-empty") || "No results.");
        if (q && !list.children.length) list.hidden = false;
      });
    };
    if (big) {
      big.value = params.get("q") || "";
      big.addEventListener("input", function () {
        run(big.value.trim());
      });
    }
    if (params.get("q")) run(params.get("q").trim());
  }
})();
