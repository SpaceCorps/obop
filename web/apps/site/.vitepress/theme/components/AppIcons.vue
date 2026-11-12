<script setup lang="ts">
import { useSite } from "../composables.ts";
import Tile from "./Tile.vue";

// A row of app icons with their names: the apps this one works with, and the related apps. The icon
// comes from the id; the name is the one the page language uses.
defineProps<{ apps: { id: string; name: string; slug: string }[] }>();
const { href } = useSite();
</script>

<template>
  <ul class="app-icons" role="list">
    <li v-for="a in apps" :key="a.id">
      <a :href="href(`/apps/${a.slug}/`)">
        <Tile fluid :id="a.id" :name="a.id" :size="64" />
        <span>{{ a.name }}</span>
      </a>
    </li>
  </ul>
</template>
