<script setup lang="ts">
import { computed } from "vue";
import { SHOWN_CARDS, drawsKicker, splitCards } from "../../../scripts/lib/cards.ts";
import AppCard from "./AppCard.vue";

interface Card {
  kicker?: string;
  /** The model an action card works on, so cards are told apart by the thing, not by its word. */
  model?: string;
  title: string;
  text: string;
  approval?: boolean;
  /** "en" when the words are English on a translated page: they are marked for screen readers. */
  lang?: string;
}

// Calm cards for what an app can do and what it keeps. A few are shown (a store page shows a few
// feature cards, not the whole manual); the rest sit in a <details>, so nothing is hidden from reading
// or search and it works without JavaScript. Which are shown, and whether the small word above the
// titles is drawn, is decided in scripts/lib/cards.ts (a card with the approval mark is never folded;
// the word is drawn on every card or on none). `more` is the label of the fold, with {n}: it names
// what is folded, so two folds on a page differ.
const props = withDefaults(defineProps<{ cards: Card[]; more: string; limit?: number }>(), {
  limit: SHOWN_CARDS,
});
const split = computed(() => splitCards(props.cards, props.limit));
const moreLabel = computed(() => props.more.replace("{n}", String(split.value.folded.length)));
const kicker = computed(() => drawsKicker(props.cards));
</script>

<template>
  <ul class="app-cards" role="list">
    <AppCard v-for="(c, i) in split.shown" :key="i" :card="c" :drawn="kicker" />
  </ul>
  <details v-if="split.folded.length" class="app-more">
    <summary>{{ moreLabel }}</summary>
    <ul class="app-cards" role="list">
      <AppCard v-for="(c, i) in split.folded" :key="i" :card="c" :drawn="kicker" />
    </ul>
  </details>
</template>
