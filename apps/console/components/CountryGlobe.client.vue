<script setup lang="ts">
import { geoContains, geoGraticule10, geoOrthographic, geoPath } from "d3-geo";
import type { DimensionStat } from "~/types/stats";
import { countryColor, type CountryGeometry } from "~/utils/countryGeo";

const props = defineProps<{
  geometry: CountryGeometry;
  countries: DimensionStat[];
  active: boolean;
  metricLabel: string;
}>();
const canvas = ref<HTMLCanvasElement | null>(null);
const rotating = ref(true);
const tooltip = ref<{
  code: string;
  label: string;
  x: number;
  y: number;
} | null>(null);
const pinned = ref(false);
const tooltipId = useId();
const heldKeys = new Set<string>();
const keyStarted = new Map<string, number>();
const keyUntil = new Map<string, number>();
const arrowKeys = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"]);
const tooltipCount = computed(() =>
  tooltip.value ? (values.value.get(tooltip.value.code) ?? 0) : 0,
);
const projection = geoOrthographic().rotate([-15, -18]).precision(0.7);
const graticule = geoGraticule10();
const values = computed(
  () =>
    new Map(props.countries.map((country) => [country.name, country.count])),
);
const maximum = computed(() =>
  Math.max(1, ...props.countries.map((country) => country.count)),
);
let width = 0,
  height = 0,
  radius = 0,
  frame = 0,
  previous = 0;
let dragging = false,
  visible = true;
const over = ref(false);
let lastPoint = [0, 0];
let startPoint = [0, 0];
let moved = false;
let resize: ResizeObserver | undefined;
let visibility: IntersectionObserver | undefined;
let motion: MediaQueryList | undefined;

function draw() {
  const context = canvas.value?.getContext("2d");
  if (!context || !width || !height) return;
  context.clearRect(0, 0, width, height);
  const path = geoPath(projection, context);
  const ocean = context.createRadialGradient(
    width * 0.4,
    height * 0.3,
    0,
    width / 2,
    height / 2,
    radius,
  );
  ocean.addColorStop(0, "#eff8ff");
  ocean.addColorStop(0.7, "#d8e9fa");
  ocean.addColorStop(1, "#98b7d5");
  context.beginPath();
  path({ type: "Sphere" });
  context.fillStyle = ocean;
  context.fill();
  context.beginPath();
  path(graticule);
  context.strokeStyle = "rgba(80,115,160,.17)";
  context.lineWidth = 0.5;
  context.stroke();
  for (const feature of props.geometry.features) {
    context.beginPath();
    path(feature);
    const value = values.value.get(feature.properties.name) ?? 0;
    context.fillStyle = value ? countryColor(value, maximum.value) : "#f0eee5";
    context.fill();
    context.strokeStyle = "rgba(255,255,255,.85)";
    context.lineWidth = 0.55;
    context.stroke();
  }
  // Limb shading adds depth while keeping country fills comparable.
  const shade = context.createRadialGradient(
    width / 2 - radius * 0.22,
    height / 2 - radius * 0.2,
    radius * 0.4,
    width / 2,
    height / 2,
    radius,
  );
  shade.addColorStop(0, "rgba(14,36,76,0)");
  shade.addColorStop(0.78, "rgba(14,36,76,.03)");
  shade.addColorStop(1, "rgba(14,36,76,.3)");
  context.beginPath();
  path({ type: "Sphere" });
  context.fillStyle = shade;
  context.fill();
}
function size() {
  if (!canvas.value) return;
  const bounds = canvas.value.getBoundingClientRect();
  width = bounds.width;
  height = bounds.height;
  radius = Math.max(1, Math.min(width, height) / 2 - 16);
  const ratio = Math.min(window.devicePixelRatio || 1, 1.5);
  canvas.value.width = Math.round(width * ratio);
  canvas.value.height = Math.round(height * ratio);
  canvas.value.getContext("2d")?.setTransform(ratio, 0, 0, ratio, 0, 0);
  projection.translate([width / 2, height / 2]).scale(radius);
  draw();
}
function clearTooltip() {
  tooltip.value = null;
  pinned.value = false;
}
function animate(time: number) {
  frame = 0;
  for (const [key, until] of keyUntil) if (until <= time) keyUntil.delete(key);
  const activeKeys = new Set([...heldKeys, ...keyUntil.keys()]);
  const keyboardActive = activeKeys.size > 0;
  const autoActive = rotating.value && !over.value && !pinned.value;
  if (
    !props.active ||
    dragging ||
    !visible ||
    document.hidden ||
    (!keyboardActive && !autoActive)
  ) {
    previous = 0;
    return;
  }
  if (!previous) previous = time;
  const elapsed = Math.min(time - previous, 50);
  // Draw on every animation frame; elapsed time keeps the speed independent
  // of display refresh rate. The 50 ms clamp prevents jumps after a long frame.
  const rotation = projection.rotate();
  const horizontal =
    Number(activeKeys.has("ArrowRight")) - Number(activeKeys.has("ArrowLeft"));
  const vertical =
    Number(activeKeys.has("ArrowUp")) - Number(activeKeys.has("ArrowDown"));
  projection.rotate([
    rotation[0] + elapsed * (keyboardActive ? horizontal * 0.06 : 360 / 10_000),
    Math.max(-80, Math.min(80, rotation[1] + elapsed * vertical * 0.06)),
  ]);
  previous = time;
  draw();
  frame = requestAnimationFrame(animate);
}
function motionChanged() {
  if (motion?.matches) rotating.value = false;
  schedule();
}
function schedule() {
  cancelAnimationFrame(frame);
  frame = 0;
  previous = 0;
  if (props.active) frame = requestAnimationFrame(animate);
}
function point(event: PointerEvent) {
  const box = canvas.value!.getBoundingClientRect();
  return [event.clientX - box.left, event.clientY - box.top];
}
function showCountry(next: number[]) {
  if (Math.hypot(next[0]! - width / 2, next[1]! - height / 2) > radius) {
    clearTooltip();
    return;
  }
  const coordinates = projection.invert?.(next as [number, number]);
  const feature =
    coordinates &&
    props.geometry.features.find((feature) =>
      geoContains(feature, coordinates),
    );
  tooltip.value = feature
    ? {
        code: feature.properties.name,
        label: feature.properties.label,
        x: Math.max(8, Math.min(width - 228, next[0]! + 14)),
        y: Math.max(8, Math.min(height - 100, next[1]! + 14)),
      }
    : null;
  if (!feature) pinned.value = false;
}
function down(event: PointerEvent) {
  if (event.button !== 0) return;
  clearKeys();
  dragging = true;
  moved = false;
  lastPoint = point(event);
  startPoint = lastPoint;
  clearTooltip();
  canvas.value?.focus({ preventScroll: true });
  canvas.value?.setPointerCapture(event.pointerId);
  schedule();
}
function move(event: PointerEvent) {
  const next = point(event);
  if (dragging) {
    if (
      !moved &&
      Math.hypot(next[0]! - startPoint[0]!, next[1]! - startPoint[1]!) < 4
    )
      return;
    moved = true;
    const rotation = projection.rotate();
    projection.rotate([
      rotation[0] + ((next[0]! - lastPoint[0]!) * 180) / radius,
      Math.max(
        -80,
        Math.min(80, rotation[1] - ((next[1]! - lastPoint[1]!) * 90) / radius),
      ),
    ]);
    lastPoint = next;
    draw();
    return;
  }
  if (!pinned.value && !heldKeys.size && !keyUntil.size) showCountry(next);
}
function up(event: PointerEvent) {
  if (!dragging) return;
  dragging = false;
  if (canvas.value?.hasPointerCapture(event.pointerId))
    canvas.value.releasePointerCapture(event.pointerId);
  if (!moved) {
    showCountry(point(event));
    pinned.value = Boolean(tooltip.value);
  }
  schedule();
}
function cancelPointer() {
  dragging = false;
  clearTooltip();
  schedule();
}
function leave() {
  over.value = false;
  if (!pinned.value) clearTooltip();
  schedule();
}
function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    clearTooltip();
    schedule();
    return;
  }
  if (!arrowKeys.has(event.key)) return;
  event.preventDefault();
  if (heldKeys.has(event.key)) return;
  heldKeys.add(event.key);
  keyStarted.set(event.key, performance.now());
  keyUntil.delete(event.key);
  rotating.value = false;
  clearTooltip();
  schedule();
}
function keyup(event: KeyboardEvent) {
  if (!arrowKeys.has(event.key)) return;
  event.preventDefault();
  if (heldKeys.delete(event.key)) {
    // A quick tap still produces a short linear movement, rather than no frame at all.
    keyUntil.set(
      event.key,
      (keyStarted.get(event.key) ?? performance.now()) + 120,
    );
    keyStarted.delete(event.key);
  }
  schedule();
}
function clearKeys() {
  heldKeys.clear();
  keyStarted.clear();
  keyUntil.clear();
}
function stopKeyboard() {
  clearKeys();
  schedule();
}
function visibilityChanged() {
  if (document.hidden) clearKeys();
  schedule();
}
function toggleRotation() {
  clearTooltip();
  rotating.value = !rotating.value;
}
watch(
  () => [props.countries, props.active, props.metricLabel, rotating.value],
  () => {
    clearTooltip();
    if (!props.active) clearKeys();
    void nextTick(() => {
      size();
      schedule();
    });
  },
);
onMounted(() => {
  motion = matchMedia("(prefers-reduced-motion: reduce)");
  if (motion.matches) rotating.value = false;
  motion.addEventListener("change", motionChanged);
  resize = new ResizeObserver(size);
  if (canvas.value) resize.observe(canvas.value);
  visibility = new IntersectionObserver((entries) => {
    visible = entries[0]?.isIntersecting ?? false;
    schedule();
  });
  if (canvas.value) visibility.observe(canvas.value);
  document.addEventListener("visibilitychange", visibilityChanged);
  window.addEventListener("blur", stopKeyboard);
  size();
  schedule();
});
onBeforeUnmount(() => {
  cancelAnimationFrame(frame);
  resize?.disconnect();
  visibility?.disconnect();
  motion?.removeEventListener("change", motionChanged);
  document.removeEventListener("visibilitychange", visibilityChanged);
  window.removeEventListener("blur", stopKeyboard);
});
</script>
<template>
  <div class="globe-shell">
    <canvas
      ref="canvas"
      tabindex="0"
      role="img"
      :aria-label="`Interactive globe shaded by country ${metricLabel.toLowerCase()}. Hover or tap a country for stats. Drag or hold arrow keys to rotate.`"
      :aria-describedby="tooltip ? tooltipId : undefined"
      @pointerdown="down"
      @pointermove="move"
      @pointerup="up"
      @pointercancel="cancelPointer"
      @lostpointercapture="dragging && cancelPointer()"
      @pointerenter="
        over = true;
        schedule();
      "
      @pointerleave="leave"
      @keydown="keyboard"
      @keyup="keyup"
      @blur="stopKeyboard"
    />
    <div
      v-if="tooltip"
      :id="tooltipId"
      class="globe-tooltip"
      role="tooltip"
      :style="{ left: `${tooltip.x}px`, top: `${tooltip.y}px` }"
    >
      <strong>{{ tooltip.label }}</strong>
      <span
        >{{ tooltipCount.toLocaleString() }}
        {{ metricLabel.toLowerCase() }}</span
      >
      <button
        v-if="pinned"
        class="globe-tooltip-close"
        type="button"
        aria-label="Close country details"
        @click="
          clearTooltip();
          schedule();
        "
      >
        ×
      </button>
    </div>
    <div class="globe-controls">
      <button
        class="button secondary compact"
        type="button"
        :aria-pressed="rotating"
        @click="toggleRotation"
      >
        {{ rotating ? "Pause rotation" : "Rotate globe" }}
      </button>
      <span>Hover or tap for stats · drag or hold arrow keys to rotate</span>
    </div>
  </div>
</template>
<style scoped>
.globe-shell {
  position: absolute;
  inset: 0;
  display: grid;
  grid-template-rows: minmax(0, 1fr) auto;
}
canvas {
  width: 100%;
  height: 100%;
  min-height: 0;
  touch-action: none;
  cursor: grab;
  filter: drop-shadow(0 12px 18px rgb(30 60 110 / 0.13));
}
canvas:active {
  cursor: grabbing;
}
canvas:focus-visible {
  outline: 2px solid var(--brand);
  outline-offset: -4px;
  border-radius: 24px;
}
.globe-tooltip {
  position: absolute;
  z-index: 2;
  display: grid;
  gap: 6px;
  width: 220px;
  max-width: calc(100% - 16px);
  box-sizing: border-box;
  padding: 14px 34px 14px 14px;
  border: 1px solid #555;
  border-radius: 12px;
  background: #111214;
  color: #f7f7f2;
  box-shadow: 0 6px 20px rgb(0 0 0 / 0.2);
  pointer-events: none;
}
.globe-tooltip strong {
  font-size: 0.85rem;
}
.globe-tooltip span {
  font-size: 0.75rem;
}
.globe-tooltip-close {
  position: absolute;
  top: 3px;
  right: 3px;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: transparent;
  color: inherit;
  font-size: 1.2rem;
  cursor: pointer;
  pointer-events: auto;
}
.globe-tooltip-close:focus-visible {
  outline: 2px solid #d8ff52;
}
.globe-controls {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: center;
  padding: 8px 0;
  min-height: 54px;
}
.globe-controls span {
  font-size: 0.8rem;
  color: var(--text-secondary);
}
@media (max-width: 600px) {
  .globe-controls span {
    width: 100%;
    text-align: center;
  }
}
</style>
