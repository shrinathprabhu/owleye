<script setup lang="ts">
import type { NuxtError } from "#app";

const props = defineProps<{ error: NuxtError }>();

const notFound = computed(() => props.error.statusCode === 404);
const title = computed(() =>
  notFound.value ? "Page not found" : "Something went wrong",
);

useHead({ title: () => `${title.value} · OWLEYE` });

function goHome() {
  clearError({ redirect: "/" });
}
</script>

<template>
  <main id="main-content" class="auth-shell">
    <section class="auth-panel" aria-labelledby="error-title">
      <BrandLockup />
      <div class="auth-copy">
        <p class="eyebrow">Error {{ error.statusCode || 500 }}</p>
        <h1 id="error-title">{{ title }}</h1>
        <p>
          {{
            notFound
              ? "This page does not exist or has moved."
              : "Please try again in a moment."
          }}
        </p>
      </div>
      <button class="button primary" type="button" @click="goHome">
        Back to dashboard
      </button>
    </section>
  </main>
</template>
