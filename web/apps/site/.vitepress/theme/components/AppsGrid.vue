<script setup lang="ts">
import apps from "../../../.generated/apps.json";
import { useSite } from "../composables.ts";
import Tile from "./Tile.vue";

withDefaults(defineProps<{ categories?: boolean; grouped?: boolean }>(), {
  categories: false,
  grouped: false,
});
const { m, href } = useSite();
const cats = Object.keys(m.value.apps.categories) as (keyof typeof m.value.apps.categories)[];
const liveApps = apps.filter((a) => a.builtin);
const marketApps = apps.filter((a) => !a.builtin);
const live = liveApps.length;
</script>

<template>
  <div class="apps-wrap">
    <h2 v-if="categories" class="sr-only">{{ m.apps.title }}</h2>
    <div class="filters">
      <fieldset class="seg">
        <legend class="sr-only">{{ m.apps.filterLabel }}</legend>
        <input id="st-all" type="radio" name="st" checked />
        <label for="st-all">{{ m.apps.all }} {{ apps.length }}</label>
        <input id="st-live" type="radio" name="st" />
        <label for="st-live">{{ m.apps.available }} {{ live }}</label>
        <input id="st-market" type="radio" name="st" />
        <label for="st-market">{{ m.apps.marketplace }} {{ apps.length - live }}</label>
      </fieldset>
      <fieldset v-if="categories" class="seg">
        <legend class="sr-only">{{ m.apps.filterLabel }}</legend>
        <input id="cat-all" type="radio" name="cat" checked />
        <label for="cat-all">{{ m.apps.all }}</label>
        <template v-for="c in cats" :key="c">
          <input :id="`cat-${c}`" type="radio" name="cat" />
          <label :for="`cat-${c}`">{{ m.apps.categories[c] }}</label>
        </template>
      </fieldset>
    </div>
    <ul v-if="!grouped" class="apps">
      <li
        v-for="a in apps"
        :key="a.id"
        class="app"
        :class="a.builtin ? 'live' : 'market'"
        :data-cat="a.category"
      >
        <div class="app-top">
          <Tile :id="a.id" :name="a.name" :size="44" />
          <span class="status">{{
            a.builtin ? m.apps.status.available : m.apps.status.marketplace
          }}</span>
        </div>
        <h3>
          <a :href="href(`/apps/${a.slug}/`)">{{ a.name }}</a>
        </h3>
        <small>{{ m.apps.taglines[a.id] ?? a.tagline }}</small>
      </li>
    </ul>
    <template v-else>
      <!-- Two tiers in two labelled groups: the group label says the status once, so the cards carry no pill. -->
      <div class="apps-group live" role="group" aria-labelledby="apps-g-live">
        <p id="apps-g-live" class="apps-group-title">
          {{ m.apps.available }} <em>{{ liveApps.length }}</em>
        </p>
        <ul class="apps featured">
          <li v-for="a in liveApps" :key="a.id" class="app live" :data-cat="a.category">
            <Tile :id="a.id" :name="a.name" :size="48" />
            <div class="app-body">
              <h3>
                <a :href="href(`/apps/${a.slug}/`)">{{ a.name }}</a>
              </h3>
              <small>{{ m.apps.taglines[a.id] ?? a.tagline }}</small>
            </div>
          </li>
        </ul>
      </div>
      <div class="apps-group market" role="group" aria-labelledby="apps-g-market">
        <p id="apps-g-market" class="apps-group-title">
          {{ m.apps.marketplace }} <em>{{ marketApps.length }}</em>
        </p>
        <ul class="apps compact">
          <li v-for="a in marketApps" :key="a.id" class="app market" :data-cat="a.category">
            <Tile :id="a.id" :name="a.name" :size="36" />
            <div class="app-body">
              <h3>
                <a :href="href(`/apps/${a.slug}/`)">{{ a.name }}</a>
              </h3>
              <small>{{ m.apps.taglines[a.id] ?? a.tagline }}</small>
            </div>
          </li>
        </ul>
      </div>
    </template>
  </div>
</template>
