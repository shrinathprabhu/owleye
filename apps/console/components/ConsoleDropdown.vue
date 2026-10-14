<script setup lang="ts" generic="T extends string | number">
import { optionsFromNodes } from "~/utils/controlOptions";

defineOptions({ inheritAttrs: false });
const props = defineProps<{
  modelValue?: T;
  value?: T;
  disabled?: boolean;
  required?: boolean;
  id?: string;
  name?: string;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: T];
  change: [event: Event];
}>();
const slots = useSlots();
const attrs = useAttrs();
const native = ref<HTMLSelectElement | null>(null);
const control = ref<{ focus: () => void } | null>(null);
const invalid = ref(false);
function choose(value: string | number) {
  if (!native.value) return;
  native.value.value = String(value);
  native.value.dispatchEvent(new Event("change", { bubbles: true }));
}
function changed(event: Event) {
  const select = event.target as HTMLSelectElement;
  const option = select.selectedOptions[0] as HTMLOptionElement & {
    _value?: T;
  };
  if (!option) return;
  invalid.value = false;
  emit("update:modelValue", (option._value ?? option.value) as T);
  emit("change", event);
}
function validate(event: Event) {
  event.preventDefault();
  invalid.value = true;
  control.value?.focus();
}
defineExpose({ focus: () => control.value?.focus() });
</script>

<template>
  <div class="console-dropdown" :class="attrs.class">
    <ConsoleSelect
      :id="id"
      ref="control"
      :model-value="modelValue ?? value ?? ''"
      :options="optionsFromNodes(slots.default?.() ?? [])"
      :disabled="disabled"
      :aria-label="attrs['aria-label'] as string | undefined"
      :labelledby="attrs['aria-labelledby'] as string | undefined"
      :describedby="attrs['aria-describedby'] as string | undefined"
      :invalid="invalid || attrs['aria-invalid'] === 'true'"
      :required="required"
      @change="choose"
    />
    <!-- Retain native option values, required validation and form submission. -->
    <select
      ref="native"
      class="console-native-select"
      tabindex="-1"
      aria-hidden="true"
      :value="modelValue ?? value"
      :disabled="disabled"
      :required="required"
      :name="name"
      @change="changed"
      @invalid="validate"
    >
      <slot />
    </select>
    <span v-if="invalid" class="console-field-error" role="alert"
      >Choose an option.</span
    >
  </div>
</template>
