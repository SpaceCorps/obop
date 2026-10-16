<script setup lang="ts">
import { computed } from "vue";
import { useData } from "vitepress";
import { useSite } from "./composables.ts";
import SiteHeader from "./components/SiteHeader.vue";
import SiteFooter from "./components/SiteFooter.vue";
import LangBanner from "./components/LangBanner.vue";
import HomePage from "./components/HomePage.vue";
import AppsIndex from "./components/AppsIndex.vue";
import AppPage from "./components/AppPage.vue";
import DocPage from "./components/DocPage.vue";
import GenericPage from "./components/GenericPage.vue";
import PricingPage from "./components/PricingPage.vue";
import SearchPage from "./components/SearchPage.vue";
import NotFound from "./components/NotFound.vue";

const { page } = useData();
const { fm, m } = useSite();

const view = computed(() => {
  if (page.value.isNotFound) return NotFound;
  switch (fm.value["layout"]) {
    case "home":
      return HomePage;
    case "apps-index":
      return AppsIndex;
    case "app":
      return AppPage;
    case "doc":
      return DocPage;
    case "pricing":
      return PricingPage;
    case "search":
      return SearchPage;
    default:
      return GenericPage;
  }
});
</script>

<template>
  <a class="skip" href="#main">{{ m.ui.skipToContent }}</a>
  <SiteHeader />
  <main id="main" tabindex="-1">
    <component :is="view" />
  </main>
  <SiteFooter />
  <LangBanner />
</template>
