<script setup lang="ts">
const props = defineProps<{ label: string; code: string }>();
const emit = defineEmits<{ copied: [] }>();
const state = ref<"idle" | "copied" | "failed">("idle");
watch(
  () => props.code,
  () => {
    state.value = "idle";
  },
);
async function copy() {
  try {
    await navigator.clipboard.writeText(props.code);
    state.value = "copied";
    emit("copied");
  } catch {
    state.value = "failed";
  }
}
</script>
<template>
  <div class="setup-code-block">
    <div class="setup-code-heading">
      <span>{{ label }}</span
      ><button type="button" @click="copy">
        {{ state === "copied" ? "Copied ✓" : "Copy code" }}
      </button>
    </div>
    <pre tabindex="0" :aria-label="label"><code>{{ code }}</code></pre>
    <span class="sr-only" role="status">{{
      state === "copied" ? "Code copied." : ""
    }}</span>
    <p v-if="state === 'failed'" role="status">
      Clipboard unavailable. Select the code to copy it manually.
    </p>
  </div>
</template>
