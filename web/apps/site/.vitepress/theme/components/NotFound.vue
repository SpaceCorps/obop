<script setup lang="ts">
import { messages } from "../../../i18n/index.ts";
import SearchBox from "./SearchBox.vue";

// A single 404.html serves every locale, so it is in English with the way to the other languages.
const m = messages.en;
const links = [
  { label: "Home", href: "/" },
  { label: "Apps", href: "/apps/" },
  { label: "Documentation", href: "/docs/" },
  { label: "Getting started", href: "/docs/getting-started/" },
  { label: "Set up agents", href: "/docs/agents/" },
  { label: "Pricing", href: "/pricing/" },
  { label: "Changelog", href: "/changelog/" },
  { label: "Community", href: "/community/" },
  { label: "Status", href: "/status/" },
];
const other = ["de", "fr", "pl", "fi", "sv", "es"] as const;
</script>

<template>
  <div class="wrap page narrow notfound">
    <p class="eyebrow-text">404</p>
    <h1>{{ m.notFound.title }}</h1>
    <p class="lead">{{ m.notFound.body }}</p>
    <SearchBox id="q-404" class="search-big" />
    <h2>{{ m.notFound.links }}</h2>
    <ul class="linklist">
      <li v-for="l in links" :key="l.href">
        <a :href="l.href">{{ l.label }}</a>
      </li>
      <li><a href="/sitemap.xml">sitemap.xml</a></li>
      <li><a href="/llms.txt">llms.txt</a></li>
    </ul>
    <p class="langs">
      <a v-for="l in other" :key="l" :href="`/${l}/`" :hreflang="messages[l].htmlLang" :lang="messages[l].htmlLang">{{
        messages[l].langName
      }}</a>
    </p>
  </div>
</template>
