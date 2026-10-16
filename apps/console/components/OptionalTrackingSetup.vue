<script setup lang="ts">
import {
  optionalTrackingSnippet,
  ANALYTICS_SDK_VERSION,
  type OptionalCollector,
} from "~/utils/optionalTrackingSnippet";
const props = defineProps<{
  siteId: string;
  apiBase: string;
  kind: OptionalCollector;
}>();
const method = ref("npm");
const snippets = computed(() =>
  optionalTrackingSnippet(props.kind, props.siteId, props.apiBase),
);
const rules = computed(() => props.kind === "rules");
</script>
<template>
  <details class="optional-tracking-setup">
    <summary>
      {{
        rules
          ? "Install tracking rules on your site"
          : "Enable Web Vitals and performance tracking"
      }}<span>View setup <span aria-hidden="true">↗</span></span>
    </summary>
    <TrackingSetupPanel
      :kicker="rules ? 'Connect your rules' : 'Measure the experience'"
      :title="
        rules
          ? 'Add events from your dashboard.'
          : 'Fast pages. Happier visitors.'
      "
      :description="
        rules
          ? 'Load the optional rules collector once, then manage click, form-submit and section-visible events here. New page loads pick up enabled rules; the standard analytics bundle does not install this collector.'
          : 'Add the optional performance entrypoint alongside your existing page analytics. Browser support and interactions determine which Web Vitals are available.'
      "
    >
      <template #intro
        ><p>
          Use one installation method. If you already use the full bundle, these
          collectors are included. Preserve your existing consent controls.
        </p></template
      >
      <div class="setup-methods" role="group" aria-label="Installation method">
        <button
          v-for="item in ['npm', 'CDN']"
          :key="item"
          type="button"
          :aria-pressed="method === item"
          @click="method = item"
        >
          {{ item }}
        </button>
      </div>
      <SetupCodeBlock
        v-if="method === 'npm'"
        label="01 · install package"
        :code="`pnpm add @owleye/analytics@${ANALYTICS_SDK_VERSION}`"
      />
      <SetupCodeBlock
        :label="
          method === 'npm' ? '02 · browser entrypoint' : '01 · add to your page'
        "
        :code="method === 'npm' ? snippets.npm : snippets.cdn"
      />
      <p class="setup-note" v-if="rules">
        New page loads fetch enabled rules. Events require the module to load
        and a matching interaction; they do not recover earlier activity.
      </p>
      <p class="setup-note" v-else-if="method === 'CDN'">
        For manual spans, call
        <code>window.OwlEyePerformance.start("load_results")</code> after the
        script loads, then call the returned function when the operation
        finishes.
      </p>
      <TrackingAssistantPrompt
        :site-id="siteId"
        :api-base="apiBase"
        :kind="kind"
      />
    </TrackingSetupPanel>
  </details>
</template>
