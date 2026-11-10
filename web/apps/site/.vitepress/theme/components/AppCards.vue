<script setup lang="ts">
import { computed } from "vue";
import AppCard from "./AppCard.vue";

interface Card {
  kicker?: string;
  title: string;
  text: string;
  approval?: boolean;
  /** "en" when the words are English on a translated page: they are marked for screen readers. */
  lang?: string;
}

// Calm cards for what an app can do and what it keeps. The first few are shown; the rest sit in a
// <details>, so nothing is hidden from reading or search and it works without JavaScript. A fold is
// only made for two cards or more ("Show 1 more actions" is not a sentence in most languages).
// `more` is the label of the fold, with {n}: it names what is folded, so two folds on a page differ.
const props = withDefaults(defineProps<{ cards: Card[]; more: string; limit?: number }>(), {
  limit: 6,
});
const shown = computed(() =>
  props.cards.length > props.limit + 1 ? props.limit : props.cards.length,
);
const first = computed(() => props.cards.slice(0, shown.value));
const rest = computed(() => props.cards.slice(shown.value));
const moreLabel = computed(() => props.more.replace("{n}", String(rest.value.length)));
</script>

<template>
  <ul class="app-cards" role="list">
    <AppCard v-for="(c, i) in first" :key="i" :card="c" />
  </ul>
  <details v-if="rest.length" class="app-more">
    <summary>{{ moreLabel }}</summary>
    <ul class="app-cards" role="list">
      <AppCard v-for="(c, i) in rest" :key="i" :card="c" />
    </ul>
  </details>
</template>
