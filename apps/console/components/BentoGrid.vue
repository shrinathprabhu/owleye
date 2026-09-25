<script setup lang="ts">
const grid = ref<HTMLElement>();
let resizeObserver: ResizeObserver | undefined;
let mutationObserver: MutationObserver | undefined;
let frame = 0;
let mounted = false;
const observed = new Set<HTMLElement>();

function scheduleLayout() {
  if (!mounted || frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    const root = grid.value;
    if (!root) return;
    const cards = new Set(
      root.querySelectorAll<HTMLElement>("[data-bento-card]"),
    );
    for (const card of observed) {
      if (!cards.has(card)) {
        resizeObserver?.unobserve(card);
        observed.delete(card);
      }
    }
    const gap = Number.parseFloat(getComputedStyle(root).columnGap) || 0;
    // Each desktop row is one pixel. Content keeps its natural height; its
    // grid span reserves that height plus the same gap used between columns.
    const sizes = [...cards].map((card) => ({
      card,
      span: Math.ceil(card.getBoundingClientRect().height + gap),
    }));
    for (const { card, span } of sizes) {
      card.style.setProperty("--bento-span", String(Math.max(1, span)));
      if (!observed.has(card)) {
        observed.add(card);
        resizeObserver?.observe(card);
      }
    }
    root.dataset.ready = "true";
  });
}

onMounted(() => {
  mounted = true;
  resizeObserver = new ResizeObserver(scheduleLayout);
  mutationObserver = new MutationObserver(scheduleLayout);
  if (grid.value) {
    resizeObserver.observe(grid.value);
    mutationObserver.observe(grid.value, { childList: true, subtree: true });
  }
  scheduleLayout();
});
onBeforeUnmount(() => {
  mounted = false;
  cancelAnimationFrame(frame);
  resizeObserver?.disconnect();
  mutationObserver?.disconnect();
});
</script>

<template>
  <div ref="grid" class="bento-grid"><slot /></div>
</template>

<style scoped>
.bento-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: var(--space-4);
}
.bento-grid[data-ready="true"] {
  grid-auto-rows: 1px;
  row-gap: 0;
}
.bento-grid[data-ready="true"] :deep([data-bento-card]) {
  grid-row-end: span var(--bento-span, 1);
}
.bento-grid :deep([data-bento-card]) {
  min-width: 0;
  align-self: start;
}
@media (max-width: 1100px) {
  .bento-grid,
  .bento-grid[data-ready="true"] {
    grid-template-columns: minmax(0, 1fr);
    grid-auto-rows: auto;
    row-gap: var(--space-4);
  }
  .bento-grid[data-ready="true"] :deep([data-bento-card]) {
    grid-row-end: auto;
  }
}
</style>
