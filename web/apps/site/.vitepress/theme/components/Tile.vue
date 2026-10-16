<script setup lang="ts">
import { computed } from "vue";
import { appTileHtml } from "@obop/ui/static";

const props = withDefaults(defineProps<{ id: string; name: string; size?: number }>(), { size: 44 });
const html = computed(() => appTileHtml(props.id, props.name, props.size));
</script>

<template>
  <span class="tile-wrap" v-html="html"></span>
</template>
