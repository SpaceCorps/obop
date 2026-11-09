<script setup lang="ts">
import { computed } from "vue";
import { useSite } from "../composables.ts";
import Icon from "./Icon.vue";

interface Card {
  kicker?: string;
  title: string;
  text: string;
  approval?: boolean;
}

// Calm cards for what an app can do and what it keeps. The first few are shown; the rest sit in a
// <details>, so nothing is hidden from reading or search and it works without JavaScript.
const props = withDefaults(defineProps<{ cards: Card[]; limit?: number }>(), { limit: 6 });
const { m, locale } = useSite();
const first = computed(() => props.cards.slice(0, props.limit));
const rest = computed(() => props.cards.slice(props.limit));
const more = computed(() => m.value.apps.store.showMore.replace("{n}", String(rest.value.length)));
// The words on the cards come from the manifests, which are English: say so on translated pages.
const en = computed(() => (locale.value === "en" ? undefined : "en"));
</script>

<template>
  <ul class="app-cards">
    <li v-for="(c, i) in first" :key="i" class="app-card">
      <p v-if="c.kicker" class="app-card-kicker" :lang="en">{{ c.kicker }}</p>
      <h3 :lang="en">{{ c.title }}</h3>
      <p class="app-card-text" :lang="en">{{ c.text }}</p>
      <p v-if="c.approval" class="app-card-note">
        <Icon name="user" :size="14" />{{ m.apps.store.needsApproval }}
      </p>
    </li>
  </ul>
  <details v-if="rest.length" class="app-more">
    <summary>{{ more }}</summary>
    <ul class="app-cards">
      <li v-for="(c, i) in rest" :key="i" class="app-card">
        <p v-if="c.kicker" class="app-card-kicker" :lang="en">{{ c.kicker }}</p>
        <h3 :lang="en">{{ c.title }}</h3>
        <p class="app-card-text" :lang="en">{{ c.text }}</p>
        <p v-if="c.approval" class="app-card-note">
          <Icon name="user" :size="14" />{{ m.apps.store.needsApproval }}
        </p>
      </li>
    </ul>
  </details>
</template>
