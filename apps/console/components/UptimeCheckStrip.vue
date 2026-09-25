<script setup lang="ts">
import type { UptimeHistory } from "~/types/uptime";
const props = defineProps<{ checks: UptimeHistory["checks"] }>();
const shown = computed(() => props.checks.slice(0, 72).reverse());
const root = ref<HTMLElement | null>(null);
const active = ref<number | null>(null);
const tabIndex = ref(0);
const pinned = ref(false);
const left = ref(0);
const tooltipId = `uptime-check-${useId()}`;
const selected = computed(() =>
  active.value === null ? null : shown.value[active.value],
);
const timestamp = (seconds: number) =>
  new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "long",
  }).format(new Date(seconds * 1000));
const label = (state: string) =>
  state === "up" ? "UP" : state === "down" ? "DOWN" : "UNKNOWN";
function show(index: number, target: EventTarget | null) {
  if (!(target instanceof HTMLElement) || !root.value) return;
  const bounds = root.value.getBoundingClientRect();
  const check = target.getBoundingClientRect();
  const width = Math.min(240, bounds.width);
  left.value = Math.max(
    0,
    Math.min(
      check.left - bounds.left + check.width / 2 - width / 2,
      bounds.width - width,
    ),
  );
  active.value = index;
}
function close() {
  active.value = null;
  pinned.value = false;
}
function leave() {
  if (!pinned.value && !root.value?.contains(document.activeElement))
    active.value = null;
}
function toggle(index: number, event: MouseEvent) {
  if (pinned.value && active.value === index) close();
  else {
    pinned.value = true;
    show(index, event.currentTarget);
  }
}
function focus(index: number, event: FocusEvent) {
  tabIndex.value = index;
  show(index, event.currentTarget);
}
function navigate(event: KeyboardEvent, index: number) {
  const next =
    event.key === "ArrowRight"
      ? Math.min(index + 1, shown.value.length - 1)
      : event.key === "ArrowLeft"
        ? Math.max(index - 1, 0)
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? shown.value.length - 1
            : null;
  if (next !== null) {
    event.preventDefault();
    pinned.value = false;
    root.value
      ?.querySelectorAll<HTMLButtonElement>(".uptime-check")
      [next]?.focus();
  }
}
function outside(event: PointerEvent) {
  if (event.target instanceof Node && !root.value?.contains(event.target))
    close();
}
watch(
  () => props.checks,
  () => {
    close();
    tabIndex.value = Math.min(tabIndex.value, shown.value.length - 1);
  },
);
onMounted(() => document.addEventListener("pointerdown", outside));
onBeforeUnmount(() => document.removeEventListener("pointerdown", outside));
</script>

<template>
  <div
    ref="root"
    class="uptime-strip"
    role="group"
    aria-label="Recent check results, oldest to newest. Use arrow keys to browse checks."
    @pointerleave="leave"
    @focusout="
      (event) => {
        if (!root?.contains(event.relatedTarget as Node)) close();
      }
    "
    @keydown.esc.prevent="close"
  >
    <button
      v-for="(check, index) in shown"
      :key="`${check.checked_at}-${index}`"
      type="button"
      class="uptime-check"
      :data-state="check.state"
      :tabindex="tabIndex === index ? 0 : -1"
      :aria-label="`${timestamp(check.checked_at)}: ${label(check.state)}`"
      :aria-describedby="active === index ? tooltipId : undefined"
      @pointerenter="
        (event) => {
          if (event.pointerType === 'mouse' && !pinned)
            show(index, event.currentTarget);
        }
      "
      @focus="focus(index, $event)"
      @click="toggle(index, $event)"
      @keydown="navigate($event, index)"
    ></button>
    <div
      v-if="selected"
      :id="tooltipId"
      role="tooltip"
      class="uptime-check-tooltip"
      :style="{ left: `${left}px` }"
    >
      <strong :data-state="selected.state"
        ><span aria-hidden="true">●</span> {{ label(selected.state) }}</strong
      >
      <time :datetime="new Date(selected.checked_at * 1000).toISOString()">{{
        timestamp(selected.checked_at)
      }}</time>
      <span class="tooltip-detail"
        >{{ selected.status_code ? `HTTP ${selected.status_code} · ` : ""
        }}{{ selected.duration_ms }} ms</span
      >
    </div>
  </div>
</template>

<style scoped>
.uptime-strip {
  position: relative;
  display: flex;
  gap: 3px;
  height: 32px;
  min-width: 0;
}
.uptime-check {
  display: block;
  flex: 1;
  min-width: 1px;
  padding: 0;
  border: 0;
  background: #a2a79c;
  border-radius: 3px;
  cursor: pointer;
}
.uptime-check[data-state="up"] {
  background: #658f36;
}
.uptime-check[data-state="down"] {
  background: #bb443a;
}
.uptime-check:hover,
.uptime-check:focus-visible {
  outline: 2px solid #262823;
  outline-offset: 2px;
}
.uptime-check-tooltip {
  position: absolute;
  z-index: 10;
  bottom: calc(100% + 10px);
  box-sizing: border-box;
  width: min(240px, 100%);
  display: grid;
  gap: 6px;
  padding: 13px 15px;
  border: 1px solid #42463c;
  border-radius: 12px;
  background: #171b16;
  color: #f7f8f3;
  box-shadow: 0 6px 18px #0003;
  font-size: 12px;
  line-height: 1.5;
}
.uptime-check-tooltip::after {
  content: "";
  position: absolute;
  top: 100%;
  width: 100%;
  height: 10px;
}
.uptime-check-tooltip strong {
  color: #d2d6c9;
  font-size: 12px;
  letter-spacing: 0.06em;
}
.uptime-check-tooltip strong[data-state="up"] {
  color: #a0e5ad;
}
.uptime-check-tooltip strong[data-state="down"] {
  color: #ffaba4;
}
.tooltip-detail {
  color: #b7bdaf;
  font-size: 11px;
}
@media (max-width: 650px) {
  .uptime-strip {
    gap: 2px;
  }
}
</style>
