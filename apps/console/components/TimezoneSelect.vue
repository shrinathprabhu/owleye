<script setup lang="ts">
import { timezoneOptions } from "~/utils/timezones";

const model = defineModel<string>({ required: true });
const id = useId();
const open = ref(false);
const query = ref("");
const active = ref(0);
const ready = ref(false);
const options = ref([{ value: model.value, label: model.value }]);
const filtered = computed(() =>
  options.value.filter((option) =>
    option.label.toLowerCase().includes(query.value.trim().toLowerCase()),
  ),
);
const selectedLabel = computed(
  () =>
    options.value.find((option) => option.value === model.value)?.label ??
    model.value,
);

onMounted(() => {
  ready.value = true;
  options.value = timezoneOptions(model.value);
});
watch(model, () => {
  if (ready.value) options.value = timezoneOptions(model.value);
  open.value = false;
});
watch(
  query,
  () => {
    active.value = 0;
  },
  { flush: "sync" },
);

function expand() {
  if (open.value) return;
  query.value = "";
  active.value = Math.max(
    0,
    options.value.findIndex((option) => option.value === model.value),
  );
  open.value = true;
  scrollActive();
}

function scrollActive() {
  nextTick(() =>
    document
      .getElementById(`${id}-option-${active.value}`)
      ?.scrollIntoView({ block: "nearest" }),
  );
}

function search(event: Event) {
  query.value = (event.target as HTMLInputElement).value;
  open.value = true;
}

function choose(value: string) {
  model.value = value;
  open.value = false;
}

function keydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (open.value) event.preventDefault();
    open.value = false;
  } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!open.value) expand();
    else
      active.value = Math.max(
        0,
        Math.min(
          filtered.value.length - 1,
          active.value + (event.key === "ArrowDown" ? 1 : -1),
        ),
      );
    scrollActive();
  } else if (event.key === "Enter" && open.value) {
    event.preventDefault();
    const option = filtered.value[active.value];
    if (option) choose(option.value);
  }
}
</script>

<template>
  <div class="timezone-field">
    <label :for="id">Timezone</label>
    <div class="timezone-control">
      <input
        :id="id"
        :value="open ? query : selectedLabel"
        role="combobox"
        aria-autocomplete="list"
        :aria-expanded="open"
        :aria-controls="`${id}-options`"
        :aria-activedescendant="
          open && filtered[active] ? `${id}-option-${active}` : undefined
        "
        :aria-describedby="`${id}-help`"
        autocomplete="off"
        :spellcheck="false"
        placeholder="Search city or timezone"
        @focus="expand"
        @click="expand"
        @input="search"
        @blur="open = false"
        @keydown="keydown"
      />
      <span class="timezone-chevron" aria-hidden="true">⌄</span>
      <div
        v-show="open"
        :id="`${id}-options`"
        class="timezone-options"
        role="listbox"
        aria-label="Timezones"
      >
        <div
          v-for="(option, index) in filtered"
          :id="`${id}-option-${index}`"
          :key="option.value"
          role="option"
          :aria-selected="option.value === model"
          :class="{ active: index === active }"
          @mousedown.prevent
          @click="choose(option.value)"
        >
          {{ option.label }}
        </div>
        <p v-if="!filtered.length" role="status">
          No matching timezones. Try a city such as Kolkata or London.
        </p>
      </div>
    </div>
    <p :id="`${id}-help`" class="timezone-help">
      Search by city or timezone. Offsets shown are current; daylight saving
      adjusts automatically.
    </p>
  </div>
</template>

<style scoped>
.timezone-field {
  grid-column: 1 / -1;
  display: grid;
  align-content: start;
  gap: 7px;
  min-width: 0;
}
.timezone-field > label {
  color: var(--text-secondary);
  font-size: 0.84rem;
  font-weight: 700;
}
.timezone-control {
  position: relative;
  min-width: 0;
}
.timezone-control input {
  width: 100%;
  padding-right: 30px;
}
.timezone-chevron {
  position: absolute;
  right: 12px;
  top: 10px;
  pointer-events: none;
}
.timezone-options {
  position: absolute;
  z-index: 40;
  top: calc(100% + 6px);
  width: 100%;
  max-height: min(300px, 40vh);
  overflow-y: auto;
  overscroll-behavior: contain;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--bg-elevated);
  color: var(--text-primary);
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.15);
  padding: 5px;
}
.timezone-options [role="option"] {
  padding: 10px;
  border-radius: 4px;
  cursor: pointer;
  overflow-wrap: anywhere;
}
.timezone-options [role="option"]:hover,
.timezone-options .active {
  background: var(--bg-subtle, #eeeae0);
  outline: 1px solid var(--brand-text-safe);
  outline-offset: -1px;
}
.timezone-options [aria-selected="true"] {
  font-weight: 750;
}
.timezone-help,
.timezone-options p {
  margin: 0;
  color: var(--text-tertiary);
  font-size: 0.75rem;
  line-height: 1.5;
}
</style>
