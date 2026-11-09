<script setup lang="ts">
import { useSite } from "../composables.ts";
import Icon from "./Icon.vue";

// One calm card: what an app can do (an action) or what it keeps (a model). The thing an action works
// on is a small line above the title, inside the heading, so a list of headings reads "Fiscal year
// Close", "Period Close" and not the same word twice. The mark that a person approves is said in words
// once, on the first card that has it (`legend`); on the others it is the same icon, with the words
// for screen readers, so the sentence (and the word "agent") is not repeated down the page.
defineProps<{
  legend?: boolean;
  card: {
    kicker?: string;
    title: string;
    text: string;
    approval?: boolean;
    /** "en" when the words are English on a translated page: they are marked for screen readers. */
    lang?: string;
  };
}>();
const { m } = useSite();
</script>

<template>
  <li class="app-card">
    <h3 :lang="card.lang">
      <span v-if="card.kicker" class="app-card-kicker">{{ card.kicker }}</span
      >{{ " " }}<span class="app-card-title">{{ card.title }}</span>
    </h3>
    <p class="app-card-text" :lang="card.lang">{{ card.text }}</p>
    <p v-if="card.approval" class="app-card-note">
      <Icon name="user" :size="16" />
      <span :class="{ 'app-sr': !legend }">{{ m.apps.store.needsApproval }}</span>
    </p>
  </li>
</template>
