<script setup lang="ts">
import { useSite } from "../composables.ts";
import Icon from "./Icon.vue";

// One calm card: what an app can do (an action) or what it keeps (a model). The thing an action works
// on is a small line above the title, inside the heading, so a list of headings reads "Fiscal year
// Close", "Period Close" and not the same word twice. A step a person approves says so in a few plain
// words next to a person icon, on every card that has it: the icon alone does not tell a reader
// what it means. The kicker always stays in the heading for screen readers; `drawn` says whether it is
// also drawn (AppCards: not when every card of the section works on the same thing).
withDefaults(
  defineProps<{
    card: {
      kicker?: string;
      title: string;
      text: string;
      approval?: boolean;
      /** "en" when the words are English on a translated page: they are marked for screen readers. */
      lang?: string;
    };
    drawn?: boolean;
  }>(),
  { drawn: true },
);
const { m } = useSite();
</script>

<template>
  <li class="app-card">
    <h3 :lang="card.lang">
      <span v-if="card.kicker" class="app-card-kicker" :class="{ 'sr-only': !drawn }">{{
        card.kicker
      }}</span
      >{{ " " }}<span class="app-card-title">{{ card.title }}</span>
    </h3>
    <p class="app-card-text" :lang="card.lang">{{ card.text }}</p>
    <p v-if="card.approval" class="app-card-note">
      <Icon name="user" :size="16" />
      <span>{{ m.apps.store.needsApproval }}</span>
    </p>
  </li>
</template>
