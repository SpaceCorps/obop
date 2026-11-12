<script setup lang="ts">
import { computed } from "vue";
import { appTileHtml } from "@obop/ui/static";
import { fluidTile } from "../../../scripts/lib/tile.ts";

// `fluid`: the page's CSS sets the size (--icon-size, in rem) and the squircle corners; `size` is then
// only the fallback. The home page's grid uses the plain fixed-size tile.
const props = withDefaults(
  defineProps<{ id: string; name: string; size?: number; fluid?: boolean }>(),
  {
    size: 44,
    fluid: false,
  },
);
const html = computed(() => {
  const tile = appTileHtml(props.id, props.name, props.size);
  return props.fluid ? fluidTile(tile, props.size) : tile;
});
</script>

<template>
  <span class="tile-wrap" v-html="html"></span>
</template>
