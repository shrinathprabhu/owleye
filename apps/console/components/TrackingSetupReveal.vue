<script setup lang="ts">
import { trackingSetupSnippet } from "~/utils/trackingSnippet";
import { ANALYTICS_SDK_VERSION } from "~/utils/optionalTrackingSnippet";
const props = defineProps<{ apiBase: string; siteId: string }>();
const isOpen = ref(false);
const recipeId = useId();
const trackingSnippet = computed(() =>
  trackingSetupSnippet(props.siteId, props.apiBase),
);
</script>
<template>
  <div class="tracking-identity">
    <span
      >Public tracking ID: <code :title="siteId">{{ siteId }}</code
      ><CopyTrackingId :value="siteId"
    /></span>
    <button
      class="tracking-toggle"
      type="button"
      :aria-controls="recipeId"
      :aria-expanded="isOpen"
      @click="isOpen = !isOpen"
    >
      {{ isOpen ? "Hide tracking code" : "Show tracking code" }}
    </button>
  </div>
  <TrackingSetupPanel
    v-if="isOpen"
    :id="recipeId"
    class="tracking-recipe-shared"
    kicker="Two tiny steps"
    title="Track the steps that lead to signup."
    description="Run this once in a browser entrypoint for page views and pathname navigation. Use track for completed product actions; add the optional rules collector for dashboard-managed events."
  >
    <SetupCodeBlock
      label="01 · install"
      :code="`pnpm add @owleye/analytics@${ANALYTICS_SDK_VERSION}`"
    />
    <SetupCodeBlock label="02 · client entry" :code="trackingSnippet" />
    <p class="tracking-help">
      Custom events accept up to 10 string, number, or boolean properties,
      including global fields.
      <a
        href="https://github.com/shrinathprabhu/owleye/blob/HEAD/packages/analytics/README.md#cdn-campaigns-custom-data-rules-and-performance"
        target="_blank"
        rel="noopener noreferrer"
        >CDN setup, manual events, campaigns, rules, and performance ↗</a
      >
    </p>
    <TrackingAssistantPrompt :site-id="siteId" :api-base="apiBase" />
  </TrackingSetupPanel>
</template>
<style scoped>
.tracking-help {
  font-size: 0.78rem;
  line-height: 1.5;
}
.tracking-help a {
  color: inherit;
  text-decoration: underline;
}

.tracking-identity {
  display: flex;
  min-width: 0;
  align-self: center;
  align-items: center;
  flex-wrap: wrap;
  gap: 7px 12px;
  margin-top: 19px;
  color: var(--text-tertiary);
  font-size: 0.8rem;
}

.tracking-identity > span {
  display: flex;
  align-items: center;
  min-width: 0;
  max-width: 100%;
  gap: 6px;
}
.tracking-identity > span > .copy-tracking {
  flex-shrink: 0;
}
.tracking-identity > span > code {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tracking-identity code {
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-size: 0.96em;
}

.tracking-toggle {
  min-height: 32px;
  border: 0;
  color: var(--brand-text-safe);
  background: transparent;
  padding: 5px 3px;
  font-size: inherit;
  font-weight: 760;
  text-decoration: underline;
  text-decoration-thickness: 1px;
  text-underline-offset: 3px;
  cursor: pointer;
}

@media (max-width: 820px) {
  .tracking-identity {
    grid-column: 1 / -1;
    grid-row: 2;
    margin-top: 0;
  }
  .tracking-recipe-shared {
    grid-row: 4;
  }
}
@media (max-width: 520px) {
  .tracking-identity,
  .tracking-recipe-shared {
    grid-column: 1;
    grid-row: auto;
  }
  .tracking-identity {
    width: 100%;
    max-width: 100%;
    overflow: hidden;
    align-items: flex-start;
    flex-direction: column;
  }
  .tracking-identity > span {
    width: 100%;
    max-width: 100%;
  }
}
</style>
