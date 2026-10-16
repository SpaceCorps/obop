<script setup lang="ts">
import { computed } from "vue";
import { iconSvg } from "@obop/ui/static";

const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 20 });
const html = computed(() => iconSvg(props.name, props.size));
</script>

<template>
  <span class="ico-wrap" v-html="html"></span>
</template>
