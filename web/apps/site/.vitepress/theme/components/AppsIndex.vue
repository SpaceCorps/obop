<script setup lang="ts">
import AppsGrid from "./AppsGrid.vue";
import PageHeader from "./PageHeader.vue";
import { useSite } from "../composables.ts";

const { m } = useSite();
</script>

<template>
  <div class="wrap page">
    <PageHeader />
    <AppsGrid :categories="true" />
    <noscript>
      <p class="fine">{{ m.apps.filterLabel }}</p>
    </noscript>
  </div>
</template>
