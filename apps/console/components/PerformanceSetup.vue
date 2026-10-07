<script setup lang="ts">
const props = defineProps<{ siteId: string; apiBase: string }>();
const copied = ref("");
const options = computed(() =>
  props.apiBase &&
  props.apiBase.replace(/\/+$/, "") !== "https://api.owleye.dev"
    ? `, { server: ${JSON.stringify(props.apiBase)} }`
    : "",
);
const npm = computed(
  () => `import { trackWebVitals, trackPerf } from "@owleye/analytics/performance";

// Initialize once in a browser entrypoint, after any required consent.
const vitals = trackWebVitals(${JSON.stringify(props.siteId)}${options.value});
// On teardown: vitals.stop();

// Optional: measure one operation explicitly.
const performance = trackPerf(${JSON.stringify(props.siteId)}${options.value});
const end = performance.start("load_results");
// ...perform the operation...
end();`,
);
const cdn = computed(
  () =>
    `<script defer src="https://cdn.jsdelivr.net/npm/@owleye/analytics@1.0.2/dist/owleye.performance.iife.js" data-owleye-id="${props.siteId}"${options.value ? ` data-owleye-server="${props.apiBase}"` : ""}><` +
    `/script>`,
);
async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    copied.value = "Copied";
  } catch {
    copied.value = "Select the code to copy it manually.";
  }
}
watch(
  () => props.siteId,
  () => {
    copied.value = "";
  },
);
</script>
<template>
  <details class="performance-setup management-card">
    <summary>Enable Web Vitals and performance tracking</summary>
    <p>
      Add the optional performance entrypoint alongside your existing page
      analytics. Choose npm or CDN once. Browser support and interactions
      determine which Web Vitals are available.
    </p>
    <h3>npm</h3>
    <button type="button" class="text-button" @click="copy(npm)">
      Copy npm code
    </button>
    <pre><code>{{ npm }}</code></pre>
    <h3>CDN</h3>
    <button type="button" class="text-button" @click="copy(cdn)">
      Copy CDN code
    </button>
    <pre><code>{{ cdn }}</code></pre>
    <p>
      For manual CDN spans, use
      <code>window.OwlEyePerformance.start("load_results")</code> after the
      script has loaded, then call the returned function when the operation
      finishes.
    </p>
    <p role="status">{{ copied }}</p>
  </details>
</template>
<style scoped>
.performance-setup {
  min-width: 0;
  margin-bottom: 24px;
}
summary {
  cursor: pointer;
  font-weight: 700;
}
pre {
  overflow-x: auto;
  max-width: 100%;
  padding: 16px;
  background: var(--surface-muted, #e9e6dc);
  border-radius: 8px;
}
p {
  line-height: 1.6;
}
</style>
