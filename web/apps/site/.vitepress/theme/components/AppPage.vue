<script setup lang="ts">
import { computed } from "vue";
import { Content } from "vitepress";
import type { AppView } from "../../../scripts/lib/app-view.ts";
import { useSite } from "../composables.ts";
import AppCards from "./AppCards.vue";
import AppIcons from "./AppIcons.vue";
import Icon from "./Icon.vue";
import Tile from "./Tile.vue";

// One app, laid out like a store page: the icon, the name, the category and one primary button; a
// strip of facts; what it is for, what you can do, what it keeps, what is safe, who it works with;
// the information list; how to get it. Everything technical is the Markdown of the page, folded
// under "For developers".
const { fm, m, href, locale } = useSite();
const s = computed(() => m.value.apps.store);
const view = computed(() => fm.value["view"] as AppView);
const builtin = computed(() => fm.value["builtin"] === true);
// The purpose text comes from data/apps.ts, in English: say so on translated pages.
const en = computed(() => (locale.value === "en" ? undefined : "en"));
// A long name (a word of 16 letters or more: Rechnungsstellung, Sähköpostimarkkinointi) is set a
// little smaller on a phone, so it fits the line whole whether or not the browser can hyphenate.
const longName = computed(() =>
  String(fm.value["title"])
    .split(/[\s-]+/)
    .some((w) => w.length >= 16),
);
// A path breaks after its slashes, not in the middle of a word.
const pathParts = (path: string) => path.split(new RegExp("(?<=/)"));
</script>

<template>
  <div class="wrap app-page">
    <p class="app-back">
      <a :href="href('/apps/')"><Icon name="chevron-left" :size="20" />{{ m.apps.title }}</a>
    </p>

    <header class="app-hero">
      <Tile fluid :id="String(fm['appId'])" :name="String(fm['appId'])" :size="120" />
      <div class="app-hero-text">
        <h1 :class="{ 'app-long': longName }">{{ fm["title"] }}</h1>
        <p class="app-tagline">{{ fm["tagline"] }}</p>
        <p class="app-category">{{ view.category }}</p>
        <a class="app-primary" :href="builtin ? href('/app/') : '#get'">{{
          builtin ? s.open : s.get
        }}</a>
      </div>
    </header>

    <div class="app-facts-box">
      <dl class="app-facts">
        <div v-for="f in view.facts" :key="f.label">
          <dt>{{ f.label }}</dt>
          <dd>{{ f.value }}</dd>
        </div>
      </dl>
    </div>
    <p v-if="view.experimental" class="app-experimental">{{ s.experimentalNote }}</p>

    <section class="app-sec">
      <h2 :id="String(fm['purposeId'])">{{ m.apps.h.purpose }}</h2>
      <p v-if="en" class="app-note">{{ s.purposeEnglish }}</p>
      <p class="app-purpose" :lang="en">{{ view.purposeLead }}</p>
      <template v-if="view.purposeRest">
        <details v-if="view.purposeFold" class="app-read">
          <summary>{{ s.readMore }}</summary>
          <p class="app-purpose-rest" :lang="en">{{ view.purposeRest }}</p>
        </details>
        <p v-else class="app-purpose-rest" :lang="en">{{ view.purposeRest }}</p>
      </template>
    </section>

    <section class="app-sec">
      <h2 id="can-do">{{ s.canDo }}</h2>
      <AppCards v-if="view.actions.length" :cards="view.actions" :more="s.showMoreActions" />
      <p v-else class="app-card app-direct">{{ s.directEdit }}</p>
    </section>

    <section class="app-sec">
      <h2 id="keeps">{{ s.keeps }}</h2>
      <AppCards :cards="view.models" :more="s.showMoreKeeps" />
    </section>

    <section class="app-sec">
      <h2 id="safe-with-agents">{{ s.safe }}</h2>
      <ul class="app-safe" role="list">
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
            <a v-if="r.href" :class="{ mono: r.mono }" :href="r.href"
              ><template v-for="(part, i) in pathParts(r.value)" :key="i"
                >{{ part }}<wbr v-if="i < pathParts(r.value).length - 1"
              /></template>
            </a>
            <span v-else :class="{ mono: r.mono }">{{ r.value }}</span>
            <span v-if="r.note" class="app-info-note"> {{ r.note }}</span>
          </dd>
        </div>
      </dl>
      <p v-if="!view.command" class="app-included">{{ s.included }}</p>
    </section>

    <section v-if="view.command" class="app-sec">
      <h2 id="get">{{ s.get }}</h2>
      <div class="app-getbox">
        <p>{{ s.getLead }}</p>
        <a class="app-pill" :href="href('/app/')">{{ s.openObop }}</a>
        <p class="app-or">{{ s.commandLine }}</p>
        <div class="language-bash">
          <button class="copy" type="button" :aria-label="m.ui.copy"></button>
          <pre><code>{{ view.command }}</code></pre>
        </div>
        <p class="fine">{{ s.getNote }}</p>
      </div>
    </section>

    <section class="app-sec">
      <h2 id="for-developers">{{ s.developers }}</h2>
      <details id="developers" class="app-dev">
        <summary>
          <span class="app-show">{{ s.developersShow }}</span
          >{{ " " }}<span class="app-hide">{{ s.developersHide }}</span>
        </summary>
        <div class="prose">
          <Content />
        </div>
      </details>
    </section>

    <section v-if="view.related.length" class="app-sec">
      <h2>{{ m.apps.h.related }}</h2>
      <AppIcons :apps="view.related" />
    </section>
  </div>
</template>
