<script setup lang="ts">
import type { ConsoleSelectOption } from "~/types/console";

const props = withDefaults(
  defineProps<{
    id?: string;
    describedby?: string;
    invalid?: boolean;
    required?: boolean;
    ariaLabel?: string;
    disabled?: boolean;
    labelledby?: string;
    modelValue: number | string;
    options: ConsoleSelectOption[];
    placeholder?: string;
  }>(),
  {
    ariaLabel: undefined,
    disabled: false,
    labelledby: undefined,
    placeholder: "Choose an option",
  },
);

const emit = defineEmits<{
  change: [value: number | string];
  "update:modelValue": [value: number | string];
}>();

const stableId = useId();
const root = ref<HTMLElement | null>(null);
const trigger = ref<HTMLButtonElement | null>(null);
const open = ref(false);
const activeIndex = ref(0);
const typeahead = ref("");
let typeaheadTimer: ReturnType<typeof setTimeout> | undefined;

const listboxId = `console-select-${stableId}`;
const triggerId = computed(() => props.id || `${listboxId}-trigger`);
const triggerLabelledby = computed(() =>
  !props.ariaLabel && props.labelledby
    ? `${props.labelledby} ${triggerId.value}`
    : undefined,
);
const selectedIndex = computed(() =>
  props.options.findIndex((option) => option.value === props.modelValue),
);
const selectedOption = computed(() => props.options[selectedIndex.value]);
const activeOption = computed(() => props.options[activeIndex.value]);
const activeOptionId = computed(() =>
  open.value && activeOption.value
    ? `${listboxId}-option-${activeIndex.value}`
    : undefined,
);

watch(
  () => props.options,
  () => {
    if (activeIndex.value >= props.options.length) {
      activeIndex.value = Math.max(0, props.options.length - 1);
    }
  },
);

const popover = ref<HTMLElement | null>(null);
const placement = ref<Record<string, string>>({});
function place() {
  const rect = trigger.value?.getBoundingClientRect();
  if (!rect) return;
  const width = Math.min(Math.max(rect.width, 290), window.innerWidth - 24);
  const below = window.innerHeight - rect.bottom - 16;
  const above = rect.top - 16;
  const up = below < 220 && above > below;
  placement.value = {
    left: `${Math.max(12, Math.min(rect.left, window.innerWidth - width - 12))}px`,
    width: `${width}px`,
    maxHeight: `${Math.max(80, Math.min(420, up ? above : below))}px`,
    top: up ? "auto" : `${rect.bottom + 8}px`,
    bottom: up ? `${window.innerHeight - rect.top + 8}px` : "auto",
  };
}
function listeners(add: boolean) {
  const method = add ? "addEventListener" : "removeEventListener";
  document[method]("pointerdown", handleOutsideClick);
  window[method]("resize", place);
  document[method]("scroll", handleScroll, true);
}
function handleScroll(event: Event) {
  if (!popover.value?.contains(event.target as Node)) place();
}
watch(open, async (value) => {
  listeners(value);
  if (value) {
    place();
    await nextTick();
    scrollActive();
  }
});
function scrollActive() {
  popover.value
    ?.querySelector<HTMLElement>(".active")
    ?.scrollIntoView({ block: "nearest" });
}
watch(activeIndex, () => nextTick(scrollActive));
watch(
  () => props.disabled,
  (value) => {
    if (value) close();
  },
);
defineExpose({ focus: () => trigger.value?.focus() });
onBeforeUnmount(() => {
  listeners(false);
  if (typeaheadTimer) clearTimeout(typeaheadTimer);
});

function handleOutsideClick(event: Event) {
  if (
    !root.value?.contains(event.target as Node) &&
    !popover.value?.contains(event.target as Node)
  )
    open.value = false;
}

function setOpen(nextOpen: boolean) {
  if (props.disabled || !props.options.length) return;
  open.value = nextOpen;
  if (nextOpen) {
    activeIndex.value =
      selectedIndex.value >= 0 && !props.options[selectedIndex.value]?.disabled
        ? selectedIndex.value
        : findEnabledIndex(0, 1);
  }
}

function close({ restoreFocus = false } = {}) {
  open.value = false;
  if (restoreFocus) nextTick(() => trigger.value?.focus());
}

function findEnabledIndex(start: number, direction: 1 | -1) {
  const optionCount = props.options.length;
  if (!optionCount) return -1;

  for (let offset = 0; offset < optionCount; offset += 1) {
    const index = (start + offset * direction + optionCount) % optionCount;
    if (!props.options[index]?.disabled) return index;
  }

  return -1;
}

function moveActive(direction: 1 | -1) {
  if (!open.value) setOpen(true);
  const nextIndex = findEnabledIndex(activeIndex.value + direction, direction);
  if (nextIndex >= 0) activeIndex.value = nextIndex;
}

function select(option: ConsoleSelectOption) {
  if (option.disabled) return;
  if (option.value !== props.modelValue) {
    emit("update:modelValue", option.value);
    emit("change", option.value);
  }
  close({ restoreFocus: true });
}

function selectActive() {
  const option = activeOption.value;
  if (option) select(option);
}

function moveToBoundary(boundary: "first" | "last") {
  if (!open.value) setOpen(true);
  const direction = boundary === "first" ? 1 : -1;
  const start = boundary === "first" ? 0 : props.options.length - 1;
  const index = findEnabledIndex(start, direction);
  if (index >= 0) activeIndex.value = index;
}

function handleTypeahead(key: string) {
  typeahead.value += key.toLocaleLowerCase();
  if (typeaheadTimer) clearTimeout(typeaheadTimer);
  typeaheadTimer = setTimeout(() => {
    typeahead.value = "";
  }, 650);

  const matchIndex = props.options.findIndex(
    (option) =>
      !option.disabled &&
      option.label.toLocaleLowerCase().startsWith(typeahead.value),
  );
  if (matchIndex >= 0) activeIndex.value = matchIndex;
}

function handleKeydown(event: KeyboardEvent) {
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      moveActive(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      moveActive(-1);
      break;
    case "Home":
      event.preventDefault();
      moveToBoundary("first");
      break;
    case "End":
      event.preventDefault();
      moveToBoundary("last");
      break;
    case "Enter":
    case " ":
      event.preventDefault();
      if (open.value) selectActive();
      else setOpen(true);
      break;
    case "Escape":
      if (open.value) {
        event.preventDefault();
        close();
      }
      break;
    case "Tab":
      close();
      break;
    default:
      if (
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        event.key.length === 1 &&
        /\S/.test(event.key)
      ) {
        if (!open.value) setOpen(true);
        handleTypeahead(event.key);
      }
  }
}
</script>

<template>
  <div ref="root" class="console-select" :class="{ open }">
    <button
      :id="triggerId"
      ref="trigger"
      class="console-select-trigger"
      type="button"
      role="combobox"
      aria-autocomplete="none"
      :aria-label="ariaLabel"
      :aria-describedby="describedby"
      :aria-invalid="invalid || undefined"
      :aria-required="required || undefined"
      :aria-activedescendant="activeOptionId"
      :aria-controls="listboxId"
      :aria-expanded="open"
      aria-haspopup="listbox"
      :aria-labelledby="triggerLabelledby"
      :disabled="disabled"
      @click="setOpen(!open)"
      @keydown="handleKeydown"
    >
      <span class="console-select-value">
        <span class="console-select-value-line">
          <strong>{{ selectedOption?.label ?? placeholder }}</strong>
          <span v-if="selectedOption?.badge" class="console-select-badge">
            {{ selectedOption.badge }}
          </span>
        </span>
        <small v-if="selectedOption?.meta || selectedOption?.description">
          {{ selectedOption.meta || selectedOption.description }}
        </small>
      </span>
      <svg
        class="console-select-chevron"
        aria-hidden="true"
        viewBox="0 0 16 16"
      >
        <path d="m3.5 6 4.5 4 4.5-4" />
      </svg>
    </button>

    <Teleport to="body">
      <div
        v-if="open"
        ref="popover"
        :style="placement"
        :id="listboxId"
        class="console-select-popover console-select-floating"
        role="listbox"
        :aria-labelledby="labelledby || triggerId"
      >
        <button
          v-for="(option, index) in options"
          :id="`${listboxId}-option-${index}`"
          :key="option.value"
          class="console-select-option"
          :class="{
            active: index === activeIndex,
            selected: option.value === modelValue,
          }"
          type="button"
          role="option"
          tabindex="-1"
          @mousedown.prevent
          :aria-disabled="option.disabled || undefined"
          :aria-selected="option.value === modelValue"
          :disabled="option.disabled"
          @click="select(option)"
          @mouseenter="activeIndex = index"
        >
          <span class="console-select-check" aria-hidden="true">
            {{ option.value === modelValue ? "✓" : "" }}
          </span>
          <span class="console-select-option-copy">
            <span class="console-select-option-line">
              <strong>{{ option.label }}</strong>
              <span v-if="option.badge" class="console-select-badge">
                {{ option.badge }}
              </span>
            </span>
            <small v-if="option.description">{{ option.description }}</small>
            <code v-if="option.meta">{{ option.meta }}</code>
          </span>
        </button>
      </div>
    </Teleport>
  </div>
</template>
