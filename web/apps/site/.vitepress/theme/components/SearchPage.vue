<script setup lang="ts">
import { useSite } from "../composables.ts";
import PageHeader from "./PageHeader.vue";
import SearchBox from "./SearchBox.vue";

const { m, locale, href } = useSite();
</script>

<template>
  <div class="wrap page narrow">
    <PageHeader />
    <SearchBox id="q-page" class="search-big" />
    <div id="search-page-results" class="search-page" :data-locale="locale" :data-empty="m.ui.searchEmpty" :data-for="m.ui.searchResultsFor"></div>
    <noscript>
      <p>{{ m.ui.searchNeedsJs }}</p>
      <ul>
        <li><a href="/sitemap.xml">sitemap.xml</a></li>
        <li><a :href="href('/docs/')">{{ m.ui.docsHome }}</a></li>
        <li><a href="/llms.txt">llms.txt</a></li>
      </ul>
    </noscript>
  </div>
</template>
