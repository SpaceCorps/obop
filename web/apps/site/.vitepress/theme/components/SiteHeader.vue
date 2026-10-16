<script setup lang="ts">
import { LOCALES, messages } from "../../../i18n/index.ts";
import { useSite } from "../composables.ts";
import Icon from "./Icon.vue";
import Logo from "./Logo.vue";
import SearchBox from "./SearchBox.vue";

const { m, href, locale, fm } = useSite();

interface Alt {
  lang: string;
  url: string;
}
function switchUrl(l: (typeof LOCALES)[number]): string {
  const alts = (fm.value["alternates"] as Alt[] | undefined) ?? [];
  const hit = alts.find((a) => a.lang === messages[l].htmlLang);
  return hit ? hit.url : l === "en" ? "/" : `/${l}/`;
}
</script>

<template>
  <header class="site-header">
    <div class="wrap bar">
      <a class="logo-link" :href="href('/')" :aria-label="m.ui.home"><Logo :size="26" /></a>
      <nav class="main-nav" :aria-label="m.ui.mainNav">
        <a v-for="n in m.nav" :key="n.href" :href="href(n.href)">{{ n.label }}</a>
      </nav>
      <SearchBox class="header-search" />
      <a class="icon-btn search-link" :href="href('/search/')" :aria-label="m.ui.search"
        ><Icon name="search" :size="18"
      /></a>
      <details class="lang">
        <summary :aria-label="m.ui.language">
          <span aria-hidden="true">{{ locale.toUpperCase() }}</span>
        </summary>
        <ul>
          <li v-for="l in LOCALES" :key="l">
            <a
              :href="switchUrl(l)"
              :hreflang="messages[l].htmlLang"
              :lang="messages[l].htmlLang"
              :aria-current="l === locale ? 'true' : undefined"
              >{{ messages[l].langName }}</a
            >
          </li>
        </ul>
      </details>
      <button class="icon-btn theme-toggle" type="button" :aria-label="m.ui.theme" hidden>
        <span class="when-light"><Icon name="moon" :size="18" /></span>
        <span class="when-dark"><Icon name="sun" :size="18" /></span>
      </button>
      <div class="head-cta">
        <a class="btn ghost" href="/app/">{{ m.ui.signIn }}</a>
        <a class="btn primary" href="/app/">{{ m.ui.startFree }}</a>
      </div>
      <details class="menu">
        <summary :aria-label="m.ui.menu"><Icon name="menu" :size="20" /></summary>
        <div class="menu-panel">
          <SearchBox id="q-menu" />
          <div class="menu-links">
            <a v-for="n in m.nav" :key="n.href" :href="href(n.href)">{{ n.label }}</a>
            <a href="/app/">{{ m.ui.signIn }}</a>
          </div>
        </div>
      </details>
    </div>
  </header>
</template>
