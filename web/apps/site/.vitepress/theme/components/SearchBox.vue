<script setup lang="ts">
import { useSite } from "../composables.ts";
import Icon from "./Icon.vue";

withDefaults(defineProps<{ id?: string }>(), { id: "q-header" });
const { m, href, locale } = useSite();
</script>

<template>
  <form class="search" role="search" :action="href('/search/')" method="get" :data-locale="locale" :data-empty="m.ui.searchEmpty">
    <label :for="id" class="sr-only">{{ m.ui.search }}</label>
    <Icon name="search" :size="16" />
    <input :id="id" name="q" type="search" autocomplete="off" :placeholder="m.ui.searchPlaceholder" />
    <ul class="search-results" hidden :aria-label="m.ui.search"></ul>
  </form>
</template>
