<script setup lang="ts">
defineProps<{
  name:
    | "overview"
    | "pro-view"
    | "ai"
    | "profile"
    | "api-keys"
    | "rules"
    | "events"
    | "funnels"
    | "campaigns"
    | "performance"
    | "users"
    | "settings"
    | "source"
    | "collapse"
    | "close"
    | "menu"
    | "sign-out";
}>();
</script>

<template>
  <svg
    class="console-nav-icon"
    aria-hidden="true"
    fill="none"
    viewBox="0 0 24 24"
  >
    <template v-if="name === 'overview'">
      <rect x="3.5" y="3.5" width="7" height="7" rx="2" />
      <rect x="13.5" y="3.5" width="7" height="4.5" rx="1.6" />
      <rect x="13.5" y="10.5" width="7" height="10" rx="2" />
      <rect x="3.5" y="13" width="7" height="7.5" rx="2" />
    </template>
    <template v-else-if="name === 'pro-view'">
      <path d="M4 19V10m5 9V5m5 14v-7m5 7V8" />
      <path d="m4 7 5-3 5 5 5-4" />
    </template>
    <template v-else-if="name === 'ai'">
      <path
        d="m12 3 1.1 3.5L16.5 8l-3.4 1.5L12 13l-1.1-3.5L7.5 8l3.4-1.5L12 3Z"
      />
      <path
        d="m18.5 13 .7 2.3 2.3.7-2.3.8-.7 2.2-.8-2.2-2.2-.8 2.2-.7.8-2.3ZM6 13l.8 2.5L9 16l-2.2.7L6 19l-.7-2.3L3 16l2.3-.5L6 13Z"
      />
    </template>
    <template v-else-if="name === 'profile'">
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21v-2a8 8 0 0 1 16 0v2" />
    </template>
    <template v-else-if="name === 'api-keys'">
      <circle cx="8" cy="8" r="5" />
      <path d="m12 12 9 9m-5-5 3-3m-1 5 3-3" />
    </template>
    <template v-else-if="name === 'rules'">
      <path d="M4 6h10m4 0h2M4 12h3m4 0h9M4 18h8m4 0h4" />
      <circle cx="16" cy="6" r="2" />
      <circle cx="9" cy="12" r="2" />
      <circle cx="14" cy="18" r="2" />
    </template>
    <template v-else-if="name === 'events'">
      <path d="M4 18V6m0 12h16" />
      <path d="m7 14 3-4 3 2 5-6" />
      <circle cx="7" cy="14" r="1" />
      <circle cx="18" cy="6" r="1" />
    </template>
    <template v-else-if="name === 'funnels'">
      <circle cx="5" cy="6" r="2" />
      <circle cx="12" cy="12" r="2" />
      <circle cx="19" cy="18" r="2" />
      <path d="m7 7.5 3.2 3M13.8 13.5l3.2 3" />
    </template>
    <template v-else-if="name === 'campaigns'">
      <path d="M4 13h3l8 5V6l-8 5H4v2Z" />
      <path d="m7 14 1 5h3l-1.4-3.4M18 9l2-1m-2 7 2 1" />
    </template>
    <template v-else-if="name === 'performance'">
      <path d="M4 17a8 8 0 1 1 16 0" />
      <path d="m12 17 4-5" />
      <circle cx="12" cy="17" r="1.5" />
    </template>
    <template v-else-if="name === 'users'">
      <circle cx="9" cy="8" r="3" />
      <path d="M3.5 19c.3-4 2.1-6 5.5-6s5.2 2 5.5 6" />
      <path d="M15 5.6a3 3 0 0 1 0 5.8M16 13.6c2.6.7 4 2.5 4.2 5.4" />
    </template>
    <template v-else-if="name === 'settings'">
      <circle cx="12" cy="12" r="3" />
      <path
        d="m19 13.5 1.3 1-.2 1.6-1.6.6-.9 1.5.2 1.7-1.5.8-1.3-1-1.8.3-.8 1.5h-1.7L10 20l-1.8-.3-1.3 1-1.5-.8.2-1.7-.9-1.5-1.6-.6-.2-1.6 1.3-1v-1.9l-1.3-1 .2-1.6 1.6-.6.9-1.5-.2-1.7 1.5-.8 1.3 1 1.8-.3.8-1.5h1.7l.8 1.5 1.8.3 1.3-1 1.5.8-.2 1.7.9 1.5 1.6.6.2 1.6-1.3 1v1.9Z"
      />
    </template>
    <template v-else-if="name === 'source'">
      <path d="M9 5H5v14h14v-4" />
      <path d="M13 5h6v6m0-6-8 8" />
    </template>
    <template v-else-if="name === 'collapse'">
      <rect x="3.5" y="4" width="17" height="16" rx="3" />
      <path d="M9 4v16m6-11-3 3 3 3" />
    </template>
    <template v-else-if="name === 'close'">
      <path d="m6 6 12 12M18 6 6 18" />
    </template>
    <template v-else-if="name === 'menu'">
      <path d="M4 7h16M4 12h16M4 17h16" />
    </template>
    <template v-else>
      <path d="M10 5H5v14h5m5-4 4-3-4-3m4 3H9" />
    </template>
  </svg>
</template>
