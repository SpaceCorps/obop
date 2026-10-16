<script setup lang="ts">
import { computed } from "vue";
import titles from "../../../.generated/titles.json";
import { useSite } from "../composables.ts";

const { fm, m, href, locale } = useSite();
const crumbs = computed(() => {
  const path = String(fm.value["pagePath"] ?? "/");
  const t = (titles as Record<string, Record<string, string>>)[locale.value] ?? {};
  const parts = path.split("/").filter(Boolean);
  const list: { name: string; url: string }[] = [{ name: "obop", url: href("/") }];
  let acc = "";
  parts.slice(0, -1).forEach((p) => {
    acc += `/${p}`;
    const title = t[`${acc}/`];
    if (title) list.push({ name: title, url: href(`${acc}/`) });
  });
  return list;
});
</script>

<template>
  <header class="page-head">
    <nav v-if="crumbs.length > 1" class="crumbs" :aria-label="m.ui.breadcrumb">
      <ol>
        <li v-for="c in crumbs" :key="c.url">
          <a :href="c.url">{{ c.name }}</a>
        </li>
      </ol>
    </nav>
    <p v-if="fm['eyebrow']" class="eyebrow-text">{{ fm["eyebrow"] }}</p>
    <h1>{{ fm["title"] }}</h1>
    <p class="lead">{{ fm["lead"] ?? fm["description"] }}</p>
    <slot />
  </header>
</template>
