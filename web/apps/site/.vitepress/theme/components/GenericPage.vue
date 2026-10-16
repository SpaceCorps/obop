<script setup lang="ts">
import { Content } from "vitepress";
import PageHeader from "./PageHeader.vue";
import UntranslatedBanner from "./UntranslatedBanner.vue";
</script>

<template>
  <div class="wrap page narrow">
    <PageHeader />
    <UntranslatedBanner />
    <div class="prose">
      <Content />
    </div>
  </div>
</template>
