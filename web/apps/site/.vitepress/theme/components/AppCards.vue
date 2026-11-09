<script setup lang="ts">
import { computed } from "vue";
import { useSite } from "../composables.ts";
import Icon from "./Icon.vue";

interface Card {
  kicker?: string;
  title: string;
  text: string;
  approval?: boolean;
  /** "en" when the words are English on a translated page: they are marked for screen readers. */
  lang?: string;
}

// Calm cards for what an app can do and what it keeps. The first few are shown; the rest sit in a
// <details>, so nothing is hidden from reading or search and it works without JavaScript.
// `more` is the label of the fold, with {n}: it names what is folded, so two folds on a page differ.
const props = withDefaults(defineProps<{ cards: Card[]; more: string; limit?: number }>(), {
  limit: 6,
});
const { m } = useSite();
const first = computed(() => props.cards.slice(0, props.limit));
const rest = computed(() => props.cards.slice(props.limit));
const moreLabel = computed(() => props.more.replace("{n}", String(rest.value.length)));
</script>

<template>
  <ul class="app-cards">
    <li v-for="(c, i) in first" :key="i" class="app-card">
      <p v-if="c.kicker" class="app-card-kicker" :lang="c.lang">{{ c.kicker }}</p>
      <h3 :lang="c.lang">{{ c.title }}</h3>
      <p class="app-card-text" :lang="c.lang">{{ c.text }}</p>
      <p v-if="c.approval" class="app-card-note">
        <Icon name="user" :size="14" />{{ m.apps.store.needsApproval }}
      </p>
    </li>
  </ul>
  <details v-if="rest.length" class="app-more">
    <summary>{{ moreLabel }}</summary>
    <ul class="app-cards">
      <li v-for="(c, i) in rest" :key="i" class="app-card">
        <p v-if="c.kicker" class="app-card-kicker" :lang="c.lang">{{ c.kicker }}</p>
        <h3 :lang="c.lang">{{ c.title }}</h3>
        <p class="app-card-text" :lang="c.lang">{{ c.text }}</p>
        <p v-if="c.approval" class="app-card-note">
          <Icon name="user" :size="14" />{{ m.apps.store.needsApproval }}
        </p>
      </li>
    </ul>
  </details>
</template>
