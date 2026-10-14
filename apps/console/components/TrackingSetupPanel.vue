<script setup lang="ts">
defineProps<{ kicker: string; title: string; description: string }>();
</script>
<template>
  <section class="setup-panel" :aria-label="title">
    <div class="setup-panel-intro">
      <p class="setup-kicker">{{ kicker }}</p>
      <h2>{{ title }}</h2>
      <p>{{ description }}</p>
      <slot name="intro" />
    </div>
    <div class="setup-panel-code"><slot /></div>
  </section>
</template>
