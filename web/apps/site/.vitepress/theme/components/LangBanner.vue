<script setup lang="ts">
import { LOCALES, messages } from "../../../i18n/index.ts";
import { useSite } from "../composables.ts";

const { locale } = useSite();
// The text of the hint in every language; /site/site.js shows it when the browser language differs.
const hints = JSON.stringify(
  Object.fromEntries(
    LOCALES.map((l) => [
      messages[l].htmlLang,
      {
        text: messages[l].ui.langSuggest,
        go: messages[l].ui.langSuggestGo,
        dismiss: messages[l].ui.dismiss,
        copy: messages[l].ui.copy,
        copied: messages[l].ui.copied,
      },
    ]),
  ),
);
</script>

<template>
  <div
    class="lang-banner"
    role="region"
    aria-label="Language"
    hidden
    :data-page-lang="messages[locale].htmlLang"
    :data-hints="hints"
  >
    <p class="lang-text"></p>
    <a class="btn primary lang-go" href="/"></a>
    <button class="btn ghost lang-dismiss" type="button"></button>
  </div>
</template>
