<script setup lang="ts">
import { useSite } from "../composables.ts";
import PageHeader from "./PageHeader.vue";

const { m } = useSite();
</script>

<template>
  <div class="wrap page">
    <PageHeader>
      <p class="planned-badge"><span class="pill warn">{{ m.pricing.plannedBadge }}</span></p>
    </PageHeader>
    <div class="plans">
      <article v-for="p in m.pricing.plans" :key="p.name" :class="{ hl: p.highlight }">
        <h2>{{ p.name }}</h2>
        <p class="price">{{ p.price }}<small>{{ p.period }}</small></p>
        <p class="blurb">{{ p.blurb }}</p>
        <ul class="plan-list">
          <li v-for="f in p.features" :key="f">{{ f }}</li>
        </ul>
      </article>
    </div>
    <p class="fine center">{{ m.pricing.note }}</p>
    <div class="narrow-block">
      <h2>{{ m.pricing.selfHostTitle }}</h2>
      <p>{{ m.pricing.selfHostBody }}</p>
      <h2>{{ m.pricing.faqTitle }}</h2>
      <div class="faq">
        <details v-for="f in m.pricing.faq" :key="f.q">
          <summary>{{ f.q }}</summary>
          <p>{{ f.a }}</p>
        </details>
      </div>
      <p><a class="btn primary large" href="/app/">{{ m.pricing.cta }}</a></p>
    </div>
  </div>
</template>
