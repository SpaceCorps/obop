<script setup lang="ts">
import { computed } from "vue";
import { useSite } from "../composables.ts";
import AppsGrid from "./AppsGrid.vue";
import Icon from "./Icon.vue";
import Logo from "./Logo.vue";
import Tile from "./Tile.vue";

const { m, href } = useSite();

/** A label the site already has (navigation or footer), found by its link target. */
const label = (to: string) =>
  m.value.nav.find((n) => n.href === to)?.label ??
  m.value.footer.groups.flatMap((g) => g.links).find((l) => l.href === to)?.label ??
  "";
/** The small caps line above each section heading. No new strings: every one is an existing label. */
const kick = computed(() => ({
  apps: label("/apps/"),
  platform: label("/platform/"),
  marketplace: label("/marketplace/"),
  steps: label("/docs/getting-started/"),
  open: m.value.home.heroPoints[0] ?? "",
  pricing: label("/pricing/"),
  faq: label("/docs/reference/faq/"),
}));
</script>

<template>
  <div class="home">
    <section class="hero">
      <canvas class="ascii" aria-hidden="true"></canvas>
      <div class="wrap hero-in">
        <div class="hero-copy">
          <p class="eyebrow">
            <span class="pill"><i></i>{{ m.home.badge }}</span> {{ m.home.eyebrow }}
          </p>
          <h1>{{ m.home.titleA }}<br />{{ m.home.titleB }}</h1>
          <p class="lead">{{ m.home.lead }}</p>
          <div class="actions">
            <a class="btn primary large" href="/app/">{{ m.home.ctaPrimary }}</a>
            <a class="btn large" :href="href('/apps/')">{{ m.home.ctaSecondary }}</a>
          </div>
        </div>

        <ul class="facts">
          <li v-for="p in m.home.heroPoints" :key="p">
            <Icon name="check-circle" :size="16" />{{ p }}
          </li>
        </ul>

        <div class="mock" aria-hidden="true">
          <div class="mock-window">
            <div class="mock-side">
              <div class="mock-app"><Tile id="crm" name="CRM" :size="28" /><b>CRM</b></div>
              <i class="on">Lead</i><i>Stage</i><i>Activity</i>
            </div>
            <div class="mock-main">
              <div class="mock-head">
                <span><small>CRM</small> <b>Lead</b></span>
                <span class="mock-new">+ New</span>
              </div>
              <div class="mock-board">
                <div class="mock-col">
                  <div class="mock-col-title"><span class="dot n"></span>New <em>2</em></div>
                  <div class="mock-card"><b>Office refit</b><span>€3,250.00</span></div>
                  <div class="mock-card"><b>Fleet telematics</b><span>€31,250.00</span></div>
                </div>
                <div class="mock-col">
                  <div class="mock-col-title"><span class="dot q"></span>Qualified <em>2</em></div>
                  <div class="mock-card"><b>Support renewal</b><span>€10,250.00</span></div>
                  <div class="mock-card"><b>Training Q3</b><span>€38,250.00</span></div>
                </div>
                <div class="mock-col">
                  <div class="mock-col-title"><span class="dot w"></span>Won <em>1</em></div>
                  <div class="mock-card"><b>Warehouse pilot</b><span>€17,250.00</span></div>
                </div>
              </div>
            </div>
          </div>
          <div class="mock-agent mock-masked">
            <p class="overline">Contact, as an agent sees it</p>
            <dl>
              <dt>Name</dt>
              <dd><code>NAME_2</code></dd>
              <dt>Email</dt>
              <dd><code>EMAIL_1</code></dd>
              <dt>Tax id</dt>
              <dd class="gone">withheld</dd>
            </dl>
          </div>
          <div class="mock-agent mock-approve">
            <Icon name="sparkle" :size="18" />
            <div>
              <b>Waiting for a human to approve</b>
              <span>post invoice INV-0042</span>
            </div>
            <span class="mock-btn">Review</span>
          </div>
          <p class="mock-caption">{{ m.home.mockCaption }}</p>
        </div>
      </div>
    </section>

    <section id="apps" class="section">
      <div class="wrap">
        <div class="sec-head">
          <div class="sec-text">
            <p class="eyebrow-text">{{ kick.apps }}</p>
            <h2>{{ m.home.appsTitle }}</h2>
            <p class="sub">{{ m.home.appsSub }}</p>
          </div>
          <div class="sec-act">
            <a class="btn" :href="href('/apps/')">{{ m.home.seeAllApps }}</a>
          </div>
        </div>
        <AppsGrid grouped />
      </div>
    </section>

    <section id="platform" class="section alt">
      <div class="wrap">
        <div class="sec-head">
          <div class="sec-text">
            <p class="eyebrow-text">{{ kick.platform }}</p>
            <h2>{{ m.home.pillarsTitle }}</h2>
            <p class="sub">{{ m.home.pillarsSub }}</p>
          </div>
        </div>
        <div class="bento">
          <article v-for="(p, i) in m.home.pillars" :key="p.title" :class="{ lead: i === 0 }">
            <span class="card-ico"><Icon :name="p.icon" :size="22" /></span>
            <h3>{{ p.title }}</h3>
            <p>{{ p.body }}</p>
            <p class="tag">{{ p.tag }}</p>
            <div v-if="i === 0" class="flow-block">
              <p id="flow-h" class="flow-label">{{ m.home.flowLabel }}</p>
              <ol class="flow" aria-labelledby="flow-h">
                <li v-for="(f, j) in m.home.flow" :key="f.title">
                  <span class="flow-n" aria-hidden="true">{{ j + 1 }}</span>
                  <b>{{ f.title }}</b>
                  <span>{{ f.body }}</span>
                </li>
              </ol>
            </div>
          </article>
        </div>
      </div>
    </section>

    <section id="marketplace" class="section alt join">
      <div class="wrap">
        <div class="split">
          <div>
            <div class="sec-text">
              <p class="eyebrow-text">{{ kick.marketplace }}</p>
              <h2>{{ m.home.marketTitle }}</h2>
              <p class="sub">{{ m.home.marketBody }}</p>
            </div>
            <ul class="ticks">
              <li v-for="t in m.home.marketTicks" :key="t">
                <Icon name="check-circle" :size="20" />{{ t }}
              </li>
            </ul>
            <p class="more-left">
              <a class="btn" :href="href('/marketplace/')">{{ m.home.marketCta }}</a>
            </p>
          </div>
          <div class="codebox">
            <div class="codebar"><i></i><i></i><i></i><span>obop.toml</span></div>
            <pre class="code" aria-label="Example obop.toml"><code><span class="c">[module]</span>
<span class="k">id</span> = <span class="s">"fleet"</span>
<span class="k">name</span> = <span class="s">"Fleet"</span>

<span class="c">[[model]]</span>
<span class="k">name</span> = <span class="s">"fleet_vehicle"</span>
<span class="k">read_group</span>  = <span class="s">"fleet.user"</span>
<span class="k">write_group</span> = <span class="s">"fleet.manager"</span>

  <span class="c">[[model.field]]</span>
  <span class="k">name</span>  = <span class="s">"driver"</span>
  <span class="k">class</span> = <span class="s">"internal"</span>
  <span class="k">personal_data</span> = <span class="n">true</span>

  <span class="c">[[model.action]]</span>
  <span class="k">name</span>  = <span class="s">"dispatch"</span>
  <span class="k">field</span> = <span class="s">"state"</span>
  <span class="k">from</span>  = <span class="s">"available"</span>
  <span class="k">to</span>    = <span class="s">"in_service"</span></code></pre>
          </div>
        </div>
      </div>
    </section>

    <section class="section">
      <div class="wrap">
        <div class="sec-head">
          <div class="sec-text">
            <p class="eyebrow-text">{{ kick.steps }}</p>
            <h2>{{ m.home.stepsTitle }}</h2>
          </div>
        </div>
        <ol class="steps">
          <li v-for="(s, i) in m.home.steps" :key="s.title">
            <b aria-hidden="true">{{ i + 1 }}</b>
            <h3>{{ s.title }}</h3>
            <p>{{ s.body }}</p>
          </li>
        </ol>
        <p class="center">
          <a class="btn primary large" href="/app/">{{ m.home.stepsCta }}</a>
        </p>
      </div>
    </section>

    <section id="open" class="section join">
      <div class="wrap">
        <div class="sec-head">
          <div class="sec-text">
            <p class="eyebrow-text">{{ kick.open }}</p>
            <h2>{{ m.home.openTitle }}</h2>
            <p class="sub">{{ m.home.openBody }}</p>
          </div>
        </div>
        <div class="cards">
          <article v-for="p in m.home.openPoints" :key="p.title">
            <h3>{{ p.title }}</h3>
            <p>{{ p.body }}</p>
          </article>
        </div>
      </div>
    </section>

    <section id="pricing" class="section alt">
      <div class="wrap">
        <div class="sec-head">
          <div class="sec-text">
            <p class="eyebrow-text">{{ kick.pricing }}</p>
            <h2>{{ m.home.pricingTitle }}</h2>
            <p class="sub">{{ m.home.pricingSub }}</p>
          </div>
        </div>
        <div class="plans">
          <article v-for="p in m.pricing.plans" :key="p.name" :class="{ hl: p.highlight }">
            <h3>{{ p.name }}</h3>
            <p class="price">
              {{ p.price }}<small>{{ p.period }}</small>
            </p>
            <p>{{ p.blurb }}</p>
          </article>
        </div>
        <p class="fine center">{{ m.home.pricingFine }}</p>
        <p class="center">
          <a class="btn" :href="href('/pricing/')">{{ m.home.pricingCta }}</a>
        </p>
      </div>
    </section>

    <section id="faq" class="section alt join">
      <div class="wrap">
        <div class="faq-split">
          <div class="sec-text">
            <p class="eyebrow-text">{{ kick.faq }}</p>
            <h2>{{ m.home.faqTitle }}</h2>
          </div>
          <div class="faq">
            <details v-for="f in m.home.faq" :key="f.q">
              <summary>{{ f.q }}</summary>
              <p>{{ f.a }}</p>
            </details>
          </div>
        </div>
      </div>
    </section>

    <section class="cta">
      <div class="wrap">
        <Logo variant="mark" :size="44" />
        <h2>{{ m.home.ctaTitle }}</h2>
        <p>{{ m.home.ctaBody }}</p>
        <a class="btn large light" href="/app/">{{ m.home.ctaButton }}</a>
      </div>
    </section>
    <component :is="'script'" type="module" src="/site/home.js" />
  </div>
</template>
