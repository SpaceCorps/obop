<script setup lang="ts">
import { computed } from "vue";
import { Content } from "vitepress";
import type { AppView } from "../../../scripts/lib/app-view.ts";
import { useSite } from "../composables.ts";
import AppCards from "./AppCards.vue";
import AppIcons from "./AppIcons.vue";
import Icon from "./Icon.vue";
import Tile from "./Tile.vue";

// One app, laid out like a store page: the icon, the name and one primary button; a strip of facts;
// what it is for, what you can do, what it keeps, what is safe, who it works with; the information
// list; how to get it. Everything technical is the Markdown of the page, folded under "For developers".
const { fm, m, href, locale } = useSite();
const s = computed(() => m.value.apps.store);
const view = computed(() => fm.value["view"] as AppView);
const builtin = computed(() => fm.value["builtin"] === true);
const category = computed(
  () => m.value.apps.categories[fm.value["category"] as keyof typeof m.value.apps.categories],
);
// The purpose text comes from data/apps.ts, in English: say so on translated pages.
const en = computed(() => (locale.value === "en" ? undefined : "en"));
</script>

<template>
  <div class="wrap app-page">
    <nav class="app-back" :aria-label="m.ui.breadcrumb">
      <a :href="href('/apps/')"><Icon name="chevron-left" :size="20" />{{ m.apps.title }}</a>
    </nav>

    <header class="app-hero">
      <Tile :id="String(fm['appId'])" :name="String(fm['title'])" :size="120" />
      <div class="app-hero-text">
        <h1>{{ fm["title"] }}</h1>
        <p class="app-tagline">{{ fm["tagline"] }}</p>
        <p class="app-category">{{ category }}</p>
        <a class="app-primary" :href="builtin ? href('/app/') : '#get'">{{
          builtin ? s.open : s.get
        }}</a>
      </div>
    </header>

    <dl class="app-facts">
      <div v-for="f in view.facts" :key="f.label">
        <dt>{{ f.label }}</dt>
        <dd>{{ f.value }}</dd>
      </div>
    </dl>

    <section class="app-sec">
      <h2 :id="String(fm['purposeId'])">{{ m.apps.h.purpose }}</h2>
      <p class="app-purpose" :lang="en">{{ fm["purpose"] }}</p>
    </section>

    <section class="app-sec">
      <h2 id="can-do">{{ s.canDo }}</h2>
      <AppCards v-if="view.actions.length" :cards="view.actions" />
      <p v-else class="app-card app-direct">{{ s.directEdit }}</p>
    </section>

    <section class="app-sec">
      <h2 id="keeps">{{ s.keeps }}</h2>
      <AppCards :cards="view.models" />
    </section>

    <section class="app-sec">
      <h2 id="safe-with-agents">{{ s.safe }}</h2>
      <ul class="app-safe">
        <li v-for="line in view.safe" :key="line">
          <Icon name="shield-check" :size="22" />{{ line }}
        </li>
      </ul>
    </section>

    <section v-if="view.worksWith.length" class="app-sec">
      <h2 id="works-with">{{ s.worksWith }}</h2>
      <AppIcons :apps="view.worksWith" />
    </section>

    <section class="app-sec">
      <h2 id="information">{{ s.info }}</h2>
      <dl class="app-info">
        <div v-for="r in view.info" :key="r.label">
          <dt>{{ r.label }}</dt>
          <dd>
            <a v-if="r.href" :class="{ mono: r.mono }" :href="r.href">{{ r.value }}</a>
            <span v-else :class="{ mono: r.mono }">{{ r.value }}</span>
            <span v-if="r.note" class="app-info-note"> {{ r.note }}</span>
          </dd>
        </div>
      </dl>
    </section>

    <section class="app-sec">
      <h2 id="get">{{ s.get }}</h2>
      <div class="app-getbox">
        <template v-if="view.command">
          <p>{{ m.apps.installPackage }}</p>
          <div class="language-bash">
            <button class="copy" type="button"></button>
            <span class="lang">bash</span>
            <pre><code>{{ view.command }}</code></pre>
          </div>
          <p class="fine">{{ m.apps.installNote }}</p>
        </template>
        <p v-else>{{ s.included }}</p>
      </div>
    </section>

    <details id="developers" class="app-dev">
      <summary>
        <h2>{{ s.developers }}</h2>
      </summary>
      <div class="prose">
        <Content />
      </div>
    </details>

    <section v-if="view.related.length" class="app-sec">
      <h2>{{ m.apps.h.related }}</h2>
      <AppIcons :apps="view.related" />
    </section>
  </div>
</template>
