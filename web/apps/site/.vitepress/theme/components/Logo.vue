<script setup lang="ts">
import { computed } from "vue";
import { logoSvg } from "@obop/ui/static";

const props = withDefaults(defineProps<{ variant?: "full" | "mark" | "word"; size?: number }>(), {
  variant: "full",
  size: 26,
});
const html = computed(() => logoSvg(props.variant, props.size, ""));
</script>

<template>
  <span class="logo-wrap" v-html="html"></span>
</template>
