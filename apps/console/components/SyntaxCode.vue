<script setup lang="ts">
import { codeTokens, type CodeLanguage } from "~/utils/codeTokens";
const props = withDefaults(
  defineProps<{ code: string; language?: CodeLanguage }>(),
  { language: "auto" },
);
const tokens = computed(() => codeTokens(props.code, props.language));
</script>

<template>
  <span class="syntax-code"
    ><span
      v-for="(token, index) in tokens"
      :key="index"
      :class="`syntax-${token.kind}`"
      >{{ token.text }}</span
    ></span
  >
</template>

<style scoped>
.syntax-code {
  color: #eeeef3;
}
.syntax-comment {
  color: #a3a8b8;
}
.syntax-string {
  color: #d8ff69;
}
.syntax-keyword,
.syntax-tag {
  color: #c4b5fd;
}
.syntax-function {
  color: #93d9ff;
}
.syntax-number {
  color: #ffbe94;
}
.syntax-attribute {
  color: #f2d28e;
}
.syntax-punctuation {
  color: #c7ccdc;
}
</style>
