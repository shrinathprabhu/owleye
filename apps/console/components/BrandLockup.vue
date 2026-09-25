<script setup lang="ts">
withDefaults(
  defineProps<{
    href?: string;
  }>(),
  {
    href: "/",
  },
);
</script>

<template>
  <a class="brand-wordmark" :href="href" aria-label="OwlEye console">
    <img
      class="brand-mark"
      src="/brand-mark.svg"
      alt=""
      width="80"
      height="80"
    />
    <span class="brand-name">OwlEye</span>

  </a>
</template>
