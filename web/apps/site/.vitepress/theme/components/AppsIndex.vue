<script setup lang="ts">
import { computed } from "vue";
import apps from "../../../.generated/apps.json";
import { useSite } from "../composables.ts";
import { softName } from "../softname.ts";
import Icon from "./Icon.vue";
import Tile from "./Tile.vue";

// The apps index reads like an app store: the six built-in apps as large tiles, the marketplace
// apps as list rows grouped by category. No filters, counts or status pills; every tile and row is
// one link (a row ends in a chevron, because it opens the app's page), so the page works and reads
// the same without JavaScript.
const { m, href, locale } = useSite();
const ready = apps.filter((a) => a.builtin);
const cats = Object.keys(m.value.apps.categories) as (keyof typeof m.value.apps.categories)[];
const groups = computed(() =>
  cats
    .map((c) => ({
      id: c,
      title: m.value.apps.categories[c],
      apps: apps.filter((a) => !a.builtin && a.category === c),
    }))
    .filter((g) => g.apps.length > 0),
);
const tagline = (id: string, fallback: string) => m.value.apps.taglines[id] ?? fallback;
// An app is called what the product calls it in the page language, else by its English name; a long
// name may break at the seams of its compound (softname.ts).
const label = (a: { name: string; names: object }) =>
  softName((a.names as Record<string, string>)[locale.value] ?? a.name);
</script>

<template>
  <div class="wrap store">
    <header class="store-head">
      <h1>{{ m.apps.title }}</h1>
      <p class="store-lead">{{ m.apps.lead }}</p>
    </header>

    <section class="store-sec" aria-labelledby="store-ready">
      <h2 id="store-ready">{{ m.apps.store.ready }}</h2>
      <ul class="store-tiles" role="list">
        <li v-for="a in ready" :key="a.id">
          <a class="store-tile" :href="href(`/apps/${a.slug}/`)">
            <Tile fluid :id="a.id" :name="a.name" :size="96" />
            <span class="store-text">
              <span class="store-name">{{ label(a) }}</span>
              <span class="store-tag">{{ tagline(a.id, a.tagline) }}</span>
            </span>
          </a>
        </li>
      </ul>
    </section>

    <section class="store-sec" aria-labelledby="store-more">
      <h2 id="store-more">{{ m.apps.store.more }}</h2>
      <div v-for="g in groups" :key="g.id" class="store-group">
        <h3>{{ g.title }}</h3>
        <ul class="store-rows" role="list">
          <li v-for="a in g.apps" :key="a.id">
            <a class="store-row" :href="href(`/apps/${a.slug}/`)">
              <Tile fluid :id="a.id" :name="a.name" :size="48" />
              <span class="store-text">
                <span class="store-name">{{ label(a) }}</span>
                <span class="store-tag">{{ tagline(a.id, a.tagline) }}</span>
              </span>
              <span class="store-chev" aria-hidden="true"
                ><Icon name="chevron-right" :size="20"
              /></span>
            </a>
          </li>
        </ul>
      </div>
    </section>
  </div>
</template>
