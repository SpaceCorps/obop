<script setup lang="ts">
import { computed } from "vue";
import { Content } from "vitepress";
import nav from "../../../.generated/docs-nav.json";
import { GITHUB, useSite } from "../composables.ts";
import Icon from "./Icon.vue";
import PageHeader from "./PageHeader.vue";
import UntranslatedBanner from "./UntranslatedBanner.vue";

interface Item {
  path: string;
  url: string;
  title: string;
  fallback: boolean;
}
interface Group {
  group: string;
  title: string;
  pages: Item[];
}
const { fm, page, m, locale } = useSite();
const groups = computed(() => (nav as unknown as Record<string, Group[]>)[locale.value] ?? []);
const flat = computed(() => groups.value.flatMap((g) => g.pages));
const idx = computed(() => flat.value.findIndex((p) => p.path === fm.value["pagePath"]));
const prev = computed(() => (idx.value > 0 ? flat.value[idx.value - 1] : undefined));
const next = computed(() => (idx.value >= 0 ? flat.value[idx.value + 1] : undefined));
const toc = computed(() =>
  (page.value.headers ?? []).filter((h) => h.level === 2 || h.level === 3),
);
const edit = computed(() => `${GITHUB}/edit/main/${fm.value["editPath"]}`);
</script>

<template>
  <div class="wrap docs">
    <details class="docs-menu">
      <summary>{{ m.ui.docsHome }}</summary>
      <div class="docs-menu-body">
        <div v-for="g in groups" :key="g.group">
          <p class="side-title">{{ g.title }}</p>
          <ul>
            <li v-for="p in g.pages" :key="p.path">
              <a :href="p.url" :aria-current="p.path === fm['pagePath'] ? 'page' : undefined">{{
                p.title
              }}</a>
            </li>
          </ul>
        </div>
      </div>
    </details>
    <div class="docs-side">
      <nav :aria-label="m.ui.docsNav">
        <div v-for="g in groups" :key="g.group">
          <p class="side-title">{{ g.title }}</p>
          <ul>
            <li v-for="p in g.pages" :key="p.path">
              <a :href="p.url" :aria-current="p.path === fm['pagePath'] ? 'page' : undefined">{{
                p.title
              }}</a>
            </li>
          </ul>
        </div>
      </nav>
    </div>
    <article class="docs-main">
      <PageHeader />
      <UntranslatedBanner />
      <div class="prose">
        <Content />
      </div>
      <footer class="doc-foot">
        <a class="edit" :href="edit"><Icon name="edit" :size="16" />{{ m.ui.editPage }}</a>
        <nav v-if="prev || next" class="prev-next" :aria-label="`${m.ui.prev} / ${m.ui.next}`">
          <a v-if="prev" class="prev" :href="prev.url" rel="prev">
            <small>{{ m.ui.prev }}</small>
            <span>{{ prev.title }}</span>
          </a>
          <a v-if="next" class="next" :href="next.url" rel="next">
            <small>{{ m.ui.next }}</small>
            <span>{{ next.title }}</span>
          </a>
        </nav>
      </footer>
    </article>
    <div v-if="toc.length > 1" class="docs-toc">
      <nav :aria-label="m.ui.onThisPage">
        <p class="side-title">{{ m.ui.onThisPage }}</p>
        <ul>
          <li v-for="h in toc" :key="h.link" :class="`l${h.level}`">
            <a :href="h.link">{{ h.title }}</a>
          </li>
        </ul>
      </nav>
    </div>
  </div>
</template>
