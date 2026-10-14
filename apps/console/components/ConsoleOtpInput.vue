<script setup lang="ts">
defineOptions({ inheritAttrs: false });
const props = withDefaults(
  defineProps<{
    modelValue: string;
    recovery?: boolean;
    allowRecovery?: boolean;
  }>(),
  { recovery: false, allowRecovery: false },
);
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const attrs = useAttrs();
const input = ref<HTMLInputElement | null>(null);
const recoveryMode = ref(false);
const plain = computed(() => props.recovery || recoveryMode.value);
const focused = ref(false);
const position = ref(0);
function selection() {
  position.value = Math.min(
    input.value?.selectionStart ?? props.modelValue.length,
    5,
  );
}
function update(event: Event) {
  const field = event.target as HTMLInputElement;
  const value = plain.value
    ? field.value
    : field.value.replace(/[^0-9]/g, "").slice(0, 6);
  if (value !== field.value) field.value = value;
  emit("update:modelValue", value);
  selection();
}
async function toggle() {
  recoveryMode.value = !recoveryMode.value;
  emit("update:modelValue", "");
  await nextTick();
  input.value?.focus();
}
defineExpose({ focus: () => input.value?.focus() });
</script>

<template>
  <div class="console-code-field">
    <div
      class="console-otp"
      :class="{ 'is-plain': plain, 'is-focused': focused }"
    >
      <input
        ref="input"
        v-bind="attrs"
        class="console-field console-otp-input"
        :value="modelValue"
        type="text"
        :inputmode="plain ? 'text' : 'numeric'"
        autocomplete="one-time-code"
        autocapitalize="none"
        :spellcheck="false"
        :maxlength="plain ? 64 : 6"
        :pattern="plain ? undefined : '[0-9]{6}'"
        :placeholder="plain ? 'Recovery code' : undefined"
        @input="update"
        @focus="
          focused = true;
          selection();
        "
        @blur="focused = false"
        @click="selection"
        @keyup="selection"
        @select="selection"
      />
      <div v-if="!plain" class="console-otp-slots" aria-hidden="true">
        <span
          v-for="digit in 6"
          :key="digit"
          :class="{
            active: focused && position === digit - 1,
            filled: !!modelValue[digit - 1],
          }"
          >{{ modelValue[digit - 1] || "·" }}</span
        >
      </div>
    </div>
    <button
      v-if="allowRecovery"
      class="text-button console-code-toggle"
      type="button"
      @click="toggle"
    >
      {{ recoveryMode ? "Use authenticator code" : "Use a recovery code" }}
    </button>
  </div>
</template>
