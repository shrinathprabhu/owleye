<script setup lang="ts">
import { trackingSetupPrompt } from "~/utils/trackingSnippet";

const props = defineProps<{ siteId: string; apiBase: string }>();
const prompt = computed(() => trackingSetupPrompt(props.siteId, props.apiBase));
const state = ref<"idle" | "copied" | "failed">("idle");
watch(prompt, () => {
  state.value = "idle";
});

async function copy() {
  try {
    await navigator.clipboard.writeText(prompt.value);
    state.value = "copied";
  } catch {
    state.value = "failed";
  }
}
</script>

<template>
  <details class="assistant-setup">
    <summary>Install with an AI coding assistant</summary>
    <p>
      Copy these instructions into your project’s coding assistant. They include
      this app’s public tracking ID and API URL. Review the changes before
      deploying.
    </p>
    <textarea
      aria-label="OwlEye installation instructions"
      :value="prompt"
      readonly
      rows="8"
      spellcheck="false"
    />
    <button type="button" @click="copy">
      {{ state === "copied" ? "Copied ✓" : "Copy instructions" }}
    </button>
    <p v-if="state !== 'idle'" role="status">
      {{
        state === "copied"
          ? "Instructions copied."
          : "Clipboard access was blocked. Select and copy the instructions above."
      }}
    </p>
  </details>
</template>

<style scoped>
.assistant-setup {
  margin-top: 16px;
  border: 1px solid #494b52;
  border-radius: 8px;
  background: #18191d;
  color: #f7f7f2;
  padding: 14px;
  min-width: 0;
}
.assistant-setup summary {
  cursor: pointer;
  font-weight: 700;
}
.assistant-setup p {
  color: #c4c5c0;
  font-size: 0.8rem;
  line-height: 1.6;
  margin: 12px 0;
}
.assistant-setup textarea {
  display: block;
  width: 100%;
  box-sizing: border-box;
  resize: vertical;
  border: 1px solid #494b52;
  border-radius: 6px;
  background: #111214;
  color: #f7f7f2;
  padding: 12px;
  font: 0.8rem/1.6 var(--font-mono);
}
.assistant-setup button {
  margin-top: 12px;
  border: 1px solid #d8ff52;
  border-radius: 6px;
  padding: 9px 12px;
  background: transparent;
  color: #d8ff52;
  cursor: pointer;
  font-weight: 700;
}
</style>
