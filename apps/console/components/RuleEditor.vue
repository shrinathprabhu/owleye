<script setup lang="ts">
import { toRaw } from "vue";
import type { ConsoleSelectOption } from "~/types/console";
import {
  defaultRuleDraft,
  TRACKING_VALUE_PATTERN,
  type RuleCustomPropertyDraft,
  type RuleDomEvent,
  type RuleDraft,
  type RuleKeyCondition,
  type RuleSelectorType,
} from "~/types/rules";

const props = withDefaults(
  defineProps<{
    busy?: boolean;
    initialDraft?: RuleDraft;
    mode?: "create" | "edit";
    readOnly?: boolean;
  }>(),
  {
    busy: false,
    initialDraft: undefined,
    mode: "create",
    readOnly: false,
  },
);

const emit = defineEmits<{
  cancel: [];
  submit: [draft: RuleDraft];
}>();

const instanceId = getCurrentInstance()?.uid ?? 0;
const draft = ref<RuleDraft>(createDraft());
const keyboardEvent = computed(() =>
  ["keydown", "keypress", "keyup"].includes(draft.value.domEvent),
);
const needsConditionValue = computed(
  () =>
    draft.value.keyCondition === "characters" ||
    draft.value.keyCondition === "debounce",
);
const submitLabel = computed(() =>
  props.mode === "edit" ? "Save rule" : "Create rule",
);
const selectorPlaceholder = computed(() => {
  if (draft.value.selectorType === "tracking") return "pricing";
  if (draft.value.selectorType === "id") return "signup-button";
  if (draft.value.selectorType === "class") return "pricing-cta";
  if (draft.value.selectorType === "text") return "Start free";
  if (draft.value.selectorType === "xpath") return "//button[@data-plan]";
  return '[data-track="signup"]';
});

const selectorOptions: ConsoleSelectOption[] = [
  {
    description: "Stable data-owleye-track attribute across builds",
    label: "Tracking attribute (recommended)",
    value: "tracking",
  },
  { description: "Match one element by its id", label: "ID", value: "id" },
  {
    description: "Match elements with one class",
    label: "Class name",
    value: "class",
  },
  {
    description: "Match visible text, case-insensitively",
    label: "Text content",
    value: "text",
  },
  {
    description: "Use a standard CSS selector",
    label: "CSS selector",
    value: "css",
  },
  { description: "Use an XPath expression", label: "XPath", value: "xpath" },
];
const eventOptions: ConsoleSelectOption[] = [
  {
    description: "A field value is committed; no value captured by default",
    label: "Field changed",
    value: "change",
  },
  {
    description: "An element receives keyboard or pointer focus",
    label: "Focus entered",
    value: "focus",
  },
  {
    description: "Focus leaves an element",
    label: "Focus left",
    value: "blur",
  },
  { description: "A key is pressed down", label: "Key down", value: "keydown" },
  {
    description: "A native audio or video element starts or resumes",
    label: "Media play",
    value: "play",
  },
  {
    description: "A native audio or video element finishes",
    label: "Media completed",
    value: "ended",
  },
  {
    description: "Buttons, links, and deliberate taps",
    label: "Click",
    value: "click",
  },
  {
    description: "A deliberate double click",
    label: "Double click",
    value: "dblclick",
  },
  {
    description: "Pointer button goes down",
    label: "Mouse down",
    value: "mousedown",
  },
  {
    description: "Pointer button comes back up",
    label: "Mouse up",
    value: "mouseup",
  },
  { description: "A key is released", label: "Key up", value: "keyup" },
  {
    description: "A printable key is pressed",
    label: "Key press",
    value: "keypress",
  },
  {
    description: "A matched form is submitted",
    label: "Form submit",
    value: "submit",
  },
  {
    description: "First entry into the viewport, including initial page load",
    label: "Scroll to section / element",
    value: "view",
  },
];
const conditionOptions: ConsoleSelectOption[] = [
  {
    description: "Track every matching key event",
    label: "No extra condition",
    value: "none",
  },
  {
    description: "Track after typing becomes quiet",
    label: "Typing settled",
    value: "debounce",
  },
  {
    description: "Track after Return or Enter",
    label: "Enter pressed",
    value: "enter",
  },
  {
    description: "Track after Escape",
    label: "Escape pressed",
    value: "escape",
  },
  {
    description: "Track after a period/full stop",
    label: "Period pressed",
    value: "period",
  },
  {
    description: "Track once input reaches X characters",
    label: "X characters",
    value: "characters",
  },
  { description: "Track after Space", label: "Space pressed", value: "space" },
];

watch(
  () => props.initialDraft,
  () => {
    draft.value = createDraft();
  },
);

watch(keyboardEvent, (enabled) => {
  if (!enabled) draft.value.keyCondition = "none";
});

const presets: Array<{
  label: string;
  event: RuleDomEvent;
  name: string;
  emittedEvent: string;
  selector: string;
  selectorType: RuleSelectorType;
}> = [
  {
    label: "CTA click",
    event: "click",
    name: "Signup button clicked",
    emittedEvent: "signup_clicked",
    selector: "signup",
    selectorType: "tracking",
  },
  {
    label: "Section reached",
    event: "view",
    name: "Pricing section viewed",
    emittedEvent: "pricing_viewed",
    selector: "pricing",
    selectorType: "tracking",
  },
  {
    label: "Form submitted",
    event: "submit",
    name: "Contact form submitted",
    emittedEvent: "contact_submitted",
    selector: "contact-form",
    selectorType: "tracking",
  },
  {
    label: "Field changed",
    event: "change",
    name: "Plan selection changed",
    emittedEvent: "plan_changed",
    selector: "plan-select",
    selectorType: "tracking",
  },
  {
    label: "Download link",
    event: "click",
    name: "Download link clicked",
    emittedEvent: "download_clicked",
    selector: "a[download]",
    selectorType: "css",
  },
];
function applyPreset(preset: (typeof presets)[number]) {
  Object.assign(draft.value, {
    domEvent: preset.event,
    name: preset.name,
    emittedEvent: preset.emittedEvent,
    selector: preset.selector,
    selectorType: preset.selectorType,
    keyCondition: "none",
  });
}

function createDraft() {
  return structuredClone(toRaw(props.initialDraft ?? defaultRuleDraft()));
}

function setSelectorType(value: number | string) {
  draft.value.selectorType = String(value) as RuleSelectorType;
}

function setDomEvent(value: number | string) {
  draft.value.domEvent = String(value) as RuleDomEvent;
}

function setCondition(value: number | string) {
  draft.value.keyCondition = String(value) as RuleKeyCondition;
}

function addCustomProperty() {
  if (draft.value.customProperties.length >= 10) return;
  draft.value.customProperties.push({
    key: "",
    selector: "",
    selectorType: "css",
  });
}

function removeCustomProperty(index: number) {
  draft.value.customProperties.splice(index, 1);
}

function setPropertySelectorType(
  property: RuleCustomPropertyDraft,
  value: number | string,
) {
  property.selectorType = String(value) as RuleSelectorType;
}

function submit() {
  if (props.readOnly || props.busy) return;
  emit("submit", structuredClone(toRaw(draft.value)));
}
</script>

<template>
  <form class="rule-editor" @submit.prevent="submit">
    <header class="rule-editor-heading">
      <div>
        <p class="panel-kicker">
          {{ mode === "edit" ? "Editing rule" : "New tracking rule" }}
        </p>
        <h2>
          {{
            mode === "edit"
              ? "Change the signal"
              : "Describe one useful interaction"
          }}
        </h2>
      </div>
      <span v-if="readOnly" class="status-badge neutral">Read only</span>
    </header>

    <div
      v-if="mode === 'create'"
      class="rule-presets"
      role="group"
      aria-label="Common tracking presets"
    >
      <span>Start with a common interaction</span>
      <button
        v-for="preset in presets"
        :key="preset.event + preset.label"
        class="button secondary compact"
        :disabled="readOnly || busy"
        type="button"
        @click="applyPreset(preset)"
      >
        {{ preset.label }}
      </button>
    </div>

    <div class="rule-editor-grid">
      <label>
        <span>Rule name</span>
        <input
          v-model="draft.name"
          autocomplete="off"
          :disabled="readOnly"
          maxlength="120"
          placeholder="Pricing signup CTA"
          required
        />
      </label>
      <label>
        <span>Event emitted to analytics</span>
        <input
          v-model="draft.emittedEvent"
          autocomplete="off"
          :disabled="readOnly"
          maxlength="120"
          pattern="[A-Za-z][A-Za-z0-9_.:-]{0,119}"
          placeholder="signup_clicked"
          required
          spellcheck="false"
        />
      </label>
      <label class="rule-field-wide">
        <span>Description <small>optional</small></span>
        <textarea
          v-model="draft.description"
          :disabled="readOnly"
          maxlength="1000"
          placeholder="Main signup call to action on the pricing page"
          rows="2"
        ></textarea>
      </label>

      <div class="rule-select-field">
        <span :id="`rule-selector-type-${instanceId}`">Selector type</span>
        <ConsoleSelect
          :disabled="readOnly"
          :labelledby="`rule-selector-type-${instanceId}`"
          :model-value="draft.selectorType"
          :options="selectorOptions"
          @change="setSelectorType"
        />
      </div>
      <label>
        <span>Selector value</span>
        <input
          v-model="draft.selector"
          autocomplete="off"
          :disabled="readOnly"
          :placeholder="selectorPlaceholder"
          :pattern="
            draft.selectorType === 'tracking'
              ? TRACKING_VALUE_PATTERN
              : undefined
          "
          :title="
            draft.selectorType === 'tracking'
              ? 'Use 1–120 letters, numbers, colons, underscores, or hyphens; start with a letter or number.'
              : undefined
          "
          required
          spellcheck="false"
        />
      </label>

      <aside class="rule-target-help rule-field-wide">
        <strong>Keep targets stable across React, Vue, and other builds</strong>
        <p>
          Add an explicit attribute to the HTML element, then choose Tracking
          attribute and enter <code>pricing</code>. Generated classes and IDs
          can change; React refs are not HTML selectors. Custom components must
          pass the attribute through to their DOM element.
        </p>
        <code class="rule-markup-example"
          >&lt;section data-owleye-track="pricing"&gt;…&lt;/section&gt;</code
        >
        <p>
          Existing stable IDs, classes, and CSS selectors still work. Adding the
          attribute requires one application update; later rule changes use that
          same target.
        </p>
      </aside>

      <div class="rule-select-field">
        <span :id="`rule-event-${instanceId}`">On interaction</span>
        <ConsoleSelect
          :disabled="readOnly"
          :labelledby="`rule-event-${instanceId}`"
          :model-value="draft.domEvent"
          :options="eventOptions"
          @change="setDomEvent"
        />
      </div>
      <p v-if="draft.domEvent === 'view'" class="field-help">
        Tracks visibility; it does not scroll the page. Counts once per matching
        element per page navigation, including elements visible on load.
        Recreated elements can count again.
      </p>
      <p v-if="draft.domEvent === 'submit'" class="field-help">
        Tracks a form submission attempt, not a confirmed successful signup or
        payment.
      </p>
      <p v-if="draft.domEvent === 'change'" class="field-help">
        Tracks a committed native field change, not every keystroke. Field
        contents are not included unless you add a custom property.
      </p>
      <div v-if="keyboardEvent" class="rule-select-field">
        <span :id="`rule-condition-${instanceId}`">Keyboard condition</span>
        <ConsoleSelect
          :disabled="readOnly"
          :labelledby="`rule-condition-${instanceId}`"
          :model-value="draft.keyCondition"
          :options="conditionOptions"
          @change="setCondition"
        />
      </div>
      <label v-if="keyboardEvent && needsConditionValue">
        <span>
          {{
            draft.keyCondition === "debounce" ? "Quiet for" : "Character count"
          }}
        </span>
        <span class="rule-number-field">
          <input
            v-model.number="draft.keyValue"
            :disabled="readOnly"
            inputmode="decimal"
            :max="draft.keyCondition === 'debounce' ? 60 : 10000"
            :min="draft.keyCondition === 'debounce' ? 0.1 : 1"
            required
            :step="draft.keyCondition === 'debounce' ? 0.1 : 1"
            type="number"
          />
          <span v-if="draft.keyCondition === 'debounce'">seconds</span>
        </span>
      </label>
      <label
        :class="{ 'rule-field-wide': !keyboardEvent || !needsConditionValue }"
      >
        <span>Page path <small>optional</small></span>
        <input
          v-model="draft.pagePath"
          autocomplete="off"
          :disabled="readOnly"
          maxlength="2048"
          pattern="/.*"
          placeholder="/pricing"
          spellcheck="false"
        />
      </label>
    </div>

    <fieldset class="rule-properties">
      <legend>Custom properties <small>optional · up to 10</small></legend>
      <p>
        OwlEye reads <code>.value</code> first, then <code>.textContent</code>.
        If neither exists, the API receives <code>null</code>. Avoid personal
        data, passwords, and free-form input.
      </p>
      <div v-if="draft.customProperties.length" class="rule-property-list">
        <div
          v-for="(property, index) in draft.customProperties"
          :key="index"
          class="rule-property-row"
        >
          <label>
            <span>Key</span>
            <input
              v-model="property.key"
              autocomplete="off"
              :disabled="readOnly"
              maxlength="64"
              placeholder="plan"
              required
            />
          </label>
          <div class="rule-select-field">
            <span :id="`rule-property-type-${instanceId}-${index}`"
              >Selector</span
            >
            <ConsoleSelect
              :disabled="readOnly"
              :labelledby="`rule-property-type-${instanceId}-${index}`"
              :model-value="property.selectorType"
              :options="selectorOptions"
              @change="setPropertySelectorType(property, $event)"
            />
          </div>
          <label>
            <span>Selector value</span>
            <input
              v-model="property.selector"
              autocomplete="off"
              :disabled="readOnly"
              :placeholder="
                property.selectorType === 'tracking'
                  ? 'plan-select'
                  : '[data-plan]'
              "
              :pattern="
                property.selectorType === 'tracking'
                  ? TRACKING_VALUE_PATTERN
                  : undefined
              "
              required
              spellcheck="false"
            />
          </label>
          <button
            class="rule-property-remove"
            :disabled="readOnly"
            type="button"
            :aria-label="`Remove custom property ${index + 1}`"
            @click="removeCustomProperty(index)"
          >
            ×
          </button>
        </div>
      </div>
      <button
        class="button secondary compact"
        :disabled="readOnly || draft.customProperties.length >= 10"
        type="button"
        @click="addCustomProperty"
      >
        + Add property
      </button>
    </fieldset>

    <div class="rule-options">
      <label class="check-field">
        <input v-model="draft.enabled" :disabled="readOnly" type="checkbox" />
        <span>Enable immediately</span>
      </label>
      <label class="check-field">
        <input
          v-model="draft.captureText"
          :disabled="readOnly"
          type="checkbox"
        />
        <span>Include matched element text</span>
      </label>
      <label class="rule-sample-field">
        <span>Sample matches</span>
        <span class="percentage-input">
          <input
            v-model.number="draft.samplePercent"
            :disabled="readOnly"
            inputmode="numeric"
            max="100"
            min="0"
            required
            step="1"
            type="number"
          />
          <span aria-hidden="true">%</span>
        </span>
      </label>
    </div>
    <p v-if="draft.captureText" class="field-help">
      Visible text can contain personal data. Use text capture only on elements
      whose content you control.
    </p>

    <footer class="rule-editor-actions">
      <button class="button primary" :disabled="busy || readOnly" type="submit">
        {{ busy ? "Saving…" : submitLabel }}
      </button>
      <button
        v-if="mode === 'edit'"
        class="button secondary"
        :disabled="busy"
        type="button"
        @click="emit('cancel')"
      >
        Cancel
      </button>
    </footer>
  </form>
</template>

<style scoped>
.rule-presets {
  display: flex;
  flex-wrap: wrap;
  gap: 0.65rem;
  align-items: center;
}
.rule-presets > span {
  flex-basis: 100%;
  font-weight: 700;
}
.rule-target-help {
  min-width: 0;
  padding: 1rem;
  border: 1px solid var(--border-default);
  border-radius: 1rem;
  background: var(--bg-subtle);
}
.rule-target-help p {
  margin: 0.65rem 0;
  font-size: 0.9rem;
}
.rule-markup-example {
  display: block;
  overflow-wrap: anywhere;
  white-space: normal;
}
</style>
