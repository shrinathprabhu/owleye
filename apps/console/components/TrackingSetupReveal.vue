<script setup lang="ts">
import { trackingSetupSnippet } from "~/utils/trackingSnippet";

const props = defineProps<{
  apiBase: string;
  siteId: string;
}>();

const isOpen = ref(false);
const copyState = ref<"idle" | "copied" | "failed">("idle");

const installSnippet = "pnpm add @owleye/analytics@1.0.2";
const trackingSnippet = computed(() =>
  trackingSetupSnippet(props.siteId, props.apiBase),
);

watch(trackingSnippet, () => {
  copyState.value = "idle";
});

watch(isOpen, (open) => {
  if (!open) copyState.value = "idle";
});

async function copyTrackingSnippet() {
  copyState.value = "idle";

  try {
    await navigator.clipboard.writeText(trackingSnippet.value);
    copyState.value = "copied";
  } catch {
    copyState.value = "failed";
  }
}
</script>

<template>
  <div class="tracking-identity">
    <span>
      Public tracking ID:
      <code :title="siteId">{{ siteId }}</code>
      <CopyTrackingId :value="siteId" />
    </span>
    <button
      class="tracking-toggle"
      type="button"
      aria-controls="tracking-recipe"
      :aria-expanded="isOpen"
      @click="isOpen = !isOpen"
    >
      {{ isOpen ? "Hide tracking code" : "Show tracking code" }}
    </button>
  </div>

  <section
    v-if="isOpen"
    id="tracking-recipe"
    class="tracking-recipe"
    aria-label="Tracking setup"
  >
    <div class="tracking-recipe-copy">
      <p class="tracking-kicker">Two tiny steps</p>
      <h2>Track the useful stuff. Skip the surveillance lore.</h2>
      <p>
        Run this once in a browser entrypoint. Page views and SPA navigation are
        automatic; use <code>track</code> for product moments you care about.
      </p>
    </div>

    <div class="tracking-snippets">
      <div class="tracking-snippet install-snippet">
        <span>01 · install</span>
        <pre><code>{{ installSnippet }}</code></pre>
      </div>
      <div class="tracking-snippet">
        <div class="tracking-snippet-heading">
          <span>02 · client entry</span>
          <button type="button" @click="copyTrackingSnippet">
            {{ copyState === "copied" ? "Copied ✓" : "Copy code" }}
          </button>
        </div>
        <span class="sr-only" aria-live="polite">
          {{
            copyState === "copied"
              ? "Tracking code copied."
              : copyState === "failed"
                ? "Clipboard access was blocked."
                : ""
          }}
        </span>
        <pre><code>{{ trackingSnippet }}</code></pre>
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
        <p v-if="copyState === 'failed'" role="status">
          Clipboard access was blocked. Select the code and copy it manually.
        </p>
      </div>
    </div>
  </section>
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

.tracking-toggle,
.tracking-snippet-heading button {
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

.tracking-recipe {
  display: grid;
  min-width: 0;
  grid-column: 1 / -1;
  grid-template-columns: minmax(220px, 0.66fr) minmax(0, 1.34fr);
  gap: clamp(18px, 3vw, 34px);
  margin-top: var(--space-1);
  border: 1.5px solid #34363d;
  border-radius: var(--radius-lg);
  color: #c4c5c0;
  background:
    radial-gradient(circle at 100% 0, rgb(61 90 254 / 0.24), transparent 18rem),
    #111214;
  padding: clamp(16px, 2.5vw, 24px);
  box-shadow: 4px 4px 0 var(--accent);
}

.tracking-recipe-copy {
  align-self: center;
}

.tracking-kicker {
  margin: 0 0 4px;
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 0.64rem;
  font-weight: 800;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

.tracking-recipe h2 {
  margin: 0;
  color: #fffef9;
  font-size: clamp(1.2rem, 2.2vw, 1.7rem);
  letter-spacing: -0.04em;
  line-height: 1.05;
}

.tracking-recipe-copy > p:last-child {
  margin: var(--space-2) 0 0;
  color: #a8a9ad;
  font-size: 0.78rem;
  line-height: 1.5;
}

.tracking-recipe-copy code {
  color: var(--accent);
  font-family: var(--font-mono);
}

.tracking-snippets {
  display: grid;
  min-width: 0;
  gap: var(--space-2);
}

.tracking-snippet {
  min-width: 0;
  border: 1px solid #34363d;
  border-radius: var(--radius-md);
  background: #18191d;
  padding: 11px 12px;
}

.tracking-snippet > span,
.tracking-snippet-heading > span {
  color: #a8a9ad;
  font-family: var(--font-mono);
  font-size: 0.62rem;
  font-weight: 760;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.tracking-snippet-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.tracking-snippet-heading button {
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 0.66rem;
}

.tracking-snippet pre {
  max-width: 100%;
  overflow: auto;
  margin: 8px 0 0;
  color: #f7f7f2;
  font-family: var(--font-mono);
  font-size: 0.72rem;
  line-height: 1.55;
  white-space: pre;
}

.tracking-snippet > p {
  margin: 7px 0 0;
  color: var(--owleye-coral);
  font-size: 0.72rem;
}

.install-snippet pre {
  color: var(--accent);
}

@media (max-width: 820px) {
  .tracking-identity {
    grid-column: 1 / -1;
    grid-row: 2;
    margin-top: 0;
  }

  .tracking-recipe {
    grid-row: 4;
    grid-template-columns: 1fr;
  }
}

@media (max-width: 520px) {
  .tracking-identity,
  .tracking-recipe {
    grid-column: 1;
    grid-row: auto;
  }

  .tracking-identity {
    width: 100%;
    max-width: 100%;
    align-items: flex-start;
    flex-direction: column;
    overflow: hidden;
  }

  .tracking-identity > span {
    display: flex;
    width: 100%;
    max-width: 100%;
  }

  .tracking-recipe {
    box-shadow: 3px 3px 0 var(--accent);
  }
}
</style>
