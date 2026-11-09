<script setup lang="ts">
import { useSite } from "../composables.ts";
import Tile from "./Tile.vue";

// A row of app icons with their names: the apps this one works with, and the related apps.
defineProps<{ apps: { id: string; name: string; slug: string }[] }>();
const { href } = useSite();
</script>

<template>
  <ul class="app-icons">
    <li v-for="a in apps" :key="a.id">
      <a :href="href(`/apps/${a.slug}/`)">
        <Tile :id="a.id" :name="a.name" :size="64" />
        <span>{{ a.name }}</span>
      </a>
    </li>
  </ul>
</template>
