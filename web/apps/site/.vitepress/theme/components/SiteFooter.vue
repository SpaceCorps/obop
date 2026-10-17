<script setup lang="ts">
import { computed } from "vue";
import { useSite, GITHUB } from "../composables.ts";
import core from "../../../.generated/core.json";
import Logo from "./Logo.vue";

const { m, href, fm } = useSite();
const md = computed(() => {
  const mdUrl = fm.value["mdUrl"] as string | undefined;
  if (!mdUrl) return undefined;
  if (fm.value["fallback"] !== true) return mdUrl;
  const p = fm.value["pagePath"] as string;
  return `${p === "/" ? "/index" : p.slice(0, -1)}.md`;
});
</script>

<template>
  <footer class="site-footer">
    <div class="wrap">
      <div class="foot-top">
        <div class="foot-brand">
          <a class="logo-link" :href="href('/')" :aria-label="m.ui.home"><Logo :size="22" /></a>
          <p>{{ m.ui.betaNote }}</p>
          <p class="fine">{{ m.ui.allRightsNote }}</p>
        </div>
        <nav class="foot-groups" :aria-label="m.ui.footerNav">
          <div v-for="g in m.footer.groups" :key="g.title">
            <h2>{{ g.title }}</h2>
            <ul>
              <li v-for="l in g.links" :key="l.href">
                <a :href="href(l.href)">{{ l.label }}</a>
              </li>
            </ul>
          </div>
        </nav>
      </div>
      <div class="foot-bottom">
        <a :href="GITHUB">{{ m.ui.sourceCode }}</a>
        <a :href="href('/changelog/')" class="core-version">obop {{ core.version }}</a>
        <a href="/llms.txt">llms.txt</a>
        <a href="/sitemap.xml">sitemap.xml</a>
        <a v-if="md" :href="md" type="text/markdown">{{ m.ui.markdownVersion }}</a>
        <a href="/.well-known/security.txt">security.txt</a>
      </div>
    </div>
  </footer>
</template>
