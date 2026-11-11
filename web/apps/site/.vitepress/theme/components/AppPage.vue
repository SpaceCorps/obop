<script setup lang="ts">
import { computed } from "vue";
import { Content } from "vitepress";
import type { AppView } from "../../../scripts/lib/app-view.ts";
import { useSite } from "../composables.ts";
import AppCards from "./AppCards.vue";
import { softName } from "../softname.ts";
import AppIcons from "./AppIcons.vue";
import Icon from "./Icon.vue";
import Tile from "./Tile.vue";

// One app, laid out like a store page: the icon, the name, one line and one primary button; a strip
// of facts; what it is for, what you can do, how to get it (a marketplace app), what it keeps, the AI
// agents and this app, who it works with; then two closed rows, "Information" and "For developers".
// Everything technical is the Markdown of the page, in the last row. Never behind a tap: the "AI
// agents and this app" section, and every card with the approval mark or `keep` (data/apps.ts: a card
// whose own words say what an AI agent may do without approval, or that a step does not do what its
// name suggests). The last row repeats the detail for developers (the package text, who sees which
// field), so it is not the only place that says it.
const { fm, m, href, locale } = useSite();
const s = computed(() => m.value.apps.store);
const view = computed(() => fm.value["view"] as AppView);
const builtin = computed(() => fm.value["builtin"] === true);
// The purpose text comes from data/apps.ts, in English: say so on translated pages.
const en = computed(() => (locale.value === "en" ? undefined : "en"));
// A long name (a word of 16 letters or more: Rechnungsstellung, Sähköpostimarkkinointi) breaks at the
// seams of its compound (softname.ts, a soft hyphen drawn only where the line breaks) instead of inside a
// word, and keeps its size on a phone; beside the icon on a tablet it is set smaller (app-page.css).
const longName = computed(() =>
  String(fm.value["title"])
    .split(/[\s-]+/)
    .some((w) => w.length >= 16),
);
// A path breaks after its slashes, not in the middle of a word.
const pathParts = (path: string) => path.split(new RegExp("(?<=/)"));
</script>

<template>
  <div class="wrap app-page" :class="{ 'app-long': longName }">
    <p class="app-back">
      <a :href="href('/apps/')"><Icon name="chevron-left" :size="20" />{{ m.apps.title }}</a>
    </p>

    <header class="app-hero">
      <Tile fluid :id="String(fm['appId'])" :name="String(fm['appId'])" :size="120" />
      <div class="app-hero-text">
        <h1>{{ softName(String(fm["title"])) }}</h1>
        <p class="app-tagline">{{ fm["tagline"] }}</p>
        <a class="app-primary" :href="builtin ? href('/docs/getting-started/') : '#get'">{{
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
    <!-- A built-in app has no Get box: this is where the page says it needs no install. -->
    <p v-if="builtin" class="app-included">{{ s.included }}</p>

    <section class="app-sec">
      <!-- One sentence needs no heading above it: the heading stays for screen readers and for old links to its id. -->
      <h2 :id="String(fm['purposeId'])" class="sr-only">{{ m.apps.h.purpose }}</h2>
      <p v-if="en" class="app-note">{{ s.purposeEnglish }}</p>
      <p class="app-purpose" :lang="en">{{ view.purposeLead }}</p>
      <p v-if="view.purposeRest" class="app-purpose-rest" :lang="en">{{ view.purposeRest }}</p>
      <details v-if="view.purposeMore" class="app-read">
        <summary>{{ s.readMore }}</summary>
        <p class="app-purpose-rest" :lang="en">{{ view.purposeMore }}</p>
      </details>
    </section>

    <section class="app-sec">
      <h2 id="can-do">{{ s.canDo }}</h2>
      <AppCards v-if="view.actions.length" :cards="view.actions" :more="s.showMoreActions" />
      <p v-else class="app-card app-direct">{{ s.directEdit }}</p>
    </section>

    <!-- The hero's button jumps here: the box follows the cards, not the whole page. -->
    <section v-if="view.command" class="app-sec">
      <h2 id="get">{{ s.get }}</h2>
      <div class="app-getbox">
        <p>{{ s.getLead }}</p>
        <a class="app-pill" :href="href('/docs/getting-started/')">{{ s.openObop }}</a>
        <!-- The command is for developers: one quiet line to tap under the filled pill, not a second pill. -->
        <details class="app-cmd">
          <summary>{{ s.commandLine }}</summary>
          <div class="language-bash">
            <button class="copy" type="button" :aria-label="m.ui.copy"></button>
            <pre><code>{{ view.command }}</code></pre>
          </div>
        </details>
        <p class="fine">{{ s.getNote }}</p>
      </div>
    </section>

    <section class="app-sec">
      <h2 id="keeps">{{ s.keeps }}</h2>
      <AppCards :cards="view.models" :more="s.showMoreKeeps" />
    </section>

    <section class="app-sec">
      <h2 id="safe-with-agents">{{ s.safe }}</h2>
      <p class="app-safe-what">{{ view.safeWhat }}</p>
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

    <!--
      What a visitor does not need on the first read: two closed rows (a store's information list and
      the part for developers). The heading is the label of its row, inside the summary, so its id,
      its size and its place in the heading order are what they were; everything is in the page, and
      print opens both. With script a link to a row's heading opens the row (site.js); without script
      it lands on the closed row, and a link to anything inside a row opens it in the browser itself.
    -->
    <div class="app-rows">
      <section>
        <details class="app-row">
          <summary>
            <h2 id="information">{{ s.info }}</h2>
          </summary>
          <div class="app-row-body">
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
          </div>
        </details>
      </section>

      <section>
        <details id="developers" class="app-row app-dev">
          <summary>
            <h2 id="for-developers">{{ s.developers }}</h2>
          </summary>
          <div class="prose">
            <Content />
          </div>
        </details>
      </section>
    </div>

    <section v-if="view.related.length" class="app-sec">
      <h2>{{ m.apps.h.related }}</h2>
      <AppIcons :apps="view.related" />
    </section>
  </div>
</template>
