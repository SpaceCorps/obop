<script setup lang="ts">
import { useSite } from "../composables.ts";

const { fm, m } = useSite();
</script>

<template>
  <div v-if="fm['fallback']" class="notice-banner" role="note">
    <strong>{{ m.ui.notTranslatedTitle }}</strong>
    {{ m.ui.notTranslatedBody }}
    <a :href="String(fm['pagePath'])" hreflang="en" lang="en">{{ m.ui.notTranslatedLink }}</a>
  </div>
</template>
