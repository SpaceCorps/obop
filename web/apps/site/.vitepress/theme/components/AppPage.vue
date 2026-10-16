<script setup lang="ts">
import { computed } from "vue";
import { Content } from "vitepress";
import apps from "../../../.generated/apps.json";
import { GITHUB, useSite } from "../composables.ts";
import PageHeader from "./PageHeader.vue";
import Tile from "./Tile.vue";

const { fm, m, href } = useSite();
const related = computed(() =>
  apps
    .filter((a) => a.category === fm.value["category"] && a.id !== fm.value["appId"])
    .slice(0, 6),
);
const edit = computed(() => `${GITHUB}/blob/main/${fm.value["editPath"]}`);
</script>

<template>
  <div class="wrap page narrow">
    <PageHeader>
      <div class="app-meta">
        <Tile :id="String(fm['appId'])" :name="String(fm['title'])" :size="56" />
        <span class="status" :class="fm['builtin'] ? 'live' : 'market'">{{
          fm["builtin"] ? m.apps.status.available : m.apps.status.marketplace
        }}</span>
        <span class="chip">{{ m.apps.categories[fm["category"] as keyof typeof m.apps.categories] }}</span>
        <span v-if="!fm['builtin']" class="chip">{{ m.apps.versionLabel }} {{ fm["version"] }}</span>
        <a class="chip link" :href="edit">manifest</a>
      </div>
    </PageHeader>
    <div class="prose">
      <Content />
    </div>
    <section v-if="related.length" class="related" :aria-label="m.apps.h.related">
      <h2>{{ m.apps.h.related }}</h2>
      <ul>
        <li v-for="a in related" :key="a.id">
          <a :href="href(`/apps/${a.slug}/`)"><Tile :id="a.id" :name="a.name" :size="28" />{{ a.name }}</a>
        </li>
      </ul>
    </section>
  </div>
</template>
