<script setup lang="ts">
import { referrerIconSource } from "~/utils/referrerIcons";

const props = defineProps<{ source?: string }>();
const source = computed(() => referrerIconSource(props.source));
const failed = ref(false);
watch(source, () => {
  failed.value = false;
});
</script>

<template>
  <span
    class="referrer-icon"
    :class="{ 'has-image': source && !failed }"
    aria-hidden="true"
  >
    <img
      v-if="source && !failed"
      :src="source"
      alt=""
      width="18"
      height="18"
      loading="lazy"
      decoding="async"
      referrerpolicy="no-referrer"
      @error="failed = true"
    />
    <svg
      v-else
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.6"
      stroke-linecap="round"
      stroke-linejoin="round"
      focusable="false"
    >
      <circle cx="12" cy="12" r="9" />
      <ellipse cx="12" cy="12" rx="3.5" ry="9" />
      <path d="M3 12h18M5.5 6.5c4 2 9 2 13 0M5.5 17.5c4-2 9-2 13 0" />
    </svg>
  </span>
</template>

<style scoped>
.referrer-icon {
  display: inline-grid;
  place-items: center;
  flex: 0 0 20px;
  width: 20px;
  height: 20px;
  overflow: hidden;
  border-radius: 5px;
  color: var(--brand-text-safe, #4057ff);
  vertical-align: -4px;
}
.referrer-icon.has-image {
  background: #fffef9;
}
.referrer-icon img {
  display: block;
  object-fit: contain;
  max-width: 18px;
  max-height: 18px;
}
.referrer-icon svg {
  display: block;
  width: 20px;
  height: 20px;
}
</style>
