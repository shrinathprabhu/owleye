<script setup lang="ts">
import type { WorkspacePermission } from "~/types/workspace";

const props = withDefaults(
  defineProps<{
    description: string;
    eyebrow?: string;
    permission?: WorkspacePermission | null;
    showSiteSelector?: boolean;
    allowEmpty?: boolean;
    title: string;
  }>(),
  {
    eyebrow: "Analytics console",
    permission: "analytics_read",
    showSiteSelector: true,
    allowEmpty: false,
  },
);

const workspace = useConsoleWorkspace();
workspace.prepareForNavigation();
const sidebarCollapsed = useState("console-sidebar-collapsed", () => false);
const allowed = computed(
  () =>
    (props.allowEmpty && !workspace.sites.value.length) ||
    !props.permission ||
    Boolean(workspace.permissions.value[props.permission]),
);
const permissionEyebrow = computed(() =>
  props.permission === "ai_use"
    ? "AI access unavailable"
    : "Read-only means read-only",
);
const permissionAdvice = computed(() => {
  if (props.permission === "ai_use") {
    return "Ask your administrator to check AI configuration for this app.";
  }
  return "Switch to an app where you are an owner or admin, or ask its owner to change your role.";
});

onMounted(() => workspace.initialize({ force: false }));
</script>

<template>
  <a class="skip-link" href="#main-content">Skip to content</a>

  <main v-if="workspace.loading.value" id="main-content" class="auth-shell">
    <section class="auth-panel auth-loading" aria-live="polite">
      <BrandLockup />
      <span class="state-orbit" aria-hidden="true"></span>
      <p>Opening your workspace…</p>
    </section>
  </main>

  <main
    v-else-if="!workspace.isAuthenticated.value"
    id="main-content"
    class="auth-shell"
  >
    <section class="auth-panel auth-loading">
      <BrandLockup />
      <template v-if="workspace.error.value">
        <p class="eyebrow">Console unavailable</p>
        <h1>Your session could not be checked.</h1>
        <p class="page-alert" role="alert">{{ workspace.error.value }}</p>
        <button
          class="button primary"
          type="button"
          @click="workspace.initialize()"
        >
          Try again
        </button>
      </template>
      <template v-else>
        <p class="eyebrow">Session required</p>
        <h1>Sign in before the charts start gossiping.</h1>
        <NuxtLink class="button primary" to="/">Go to sign in</NuxtLink>
      </template>
    </section>
  </main>

  <div
    v-else
    class="console-shell"
    :class="{ 'sidebar-collapsed': sidebarCollapsed }"
  >
    <ConsoleSidebar
      v-if="workspace.user.value"
      v-model:collapsed="sidebarCollapsed"
      :current-role="workspace.currentRole.value"
      :demo="workspace.isDemoUser.value"
      :entitlements="workspace.selectedSite.value?.entitlements"
      :permissions="workspace.permissions.value"
      :site-id="workspace.selectedSite.value?.tracking_id"
      :sign-out-pending="workspace.signOutPending.value"
      :user="workspace.user.value"
      @sign-out="workspace.signOut"
    />

    <main id="main-content" class="workspace section-workspace">
      <AppNavigation
        v-if="showSiteSelector"
        :model-value="workspace.selectedSiteId.value"
        :options="workspace.siteOptions.value"
        @change="workspace.selectSite"
      />
      <header class="page-header section-page-header">
        <div class="page-title">
          <p class="eyebrow">{{ eyebrow }}</p>
          <h1>{{ title }}</h1>
          <p>{{ description }}</p>
        </div>
      </header>

      <p
        v-if="
          workspace.error.value && (workspace.sites.value.length || allowEmpty)
        "
        class="page-alert"
        role="alert"
      >
        {{ workspace.error.value }}
      </p>

      <section
        v-if="
          !workspace.sites.value.length && workspace.error.value && !allowEmpty
        "
        class="site-empty"
      >
        <div>
          <p class="eyebrow">App catalog unavailable</p>
          <h2>Your session is still valid.</h2>
          <p>Retry the app list without signing in again.</p>
          <button
            class="button primary compact"
            type="button"
            @click="workspace.initialize()"
          >
            Retry apps
          </button>
        </div>
      </section>

      <section
        v-else-if="!workspace.sites.value.length && !allowEmpty"
        class="site-empty"
      >
        <div>
          <p class="eyebrow">No apps yet</p>
          <h2>Create your first app.</h2>
          <p>
            The console needs a scoped tracking ID before this page can work.
          </p>
          <NuxtLink class="button primary compact" to="/app/create"
            >Create app</NuxtLink
          >
        </div>
      </section>

      <section v-else-if="!allowed" class="permission-panel" role="status">
        <span class="permission-symbol" aria-hidden="true">◌</span>
        <div>
          <p class="eyebrow">{{ permissionEyebrow }}</p>
          <h2>This app does not grant access to {{ title.toLowerCase() }}.</h2>
          <p>{{ permissionAdvice }}</p>
        </div>
      </section>

      <slot
        v-else
        :api-base="workspace.apiBase.value"
        :demo="workspace.isDemoUser.value"
        :permissions="workspace.permissions.value"
        :site="workspace.selectedSite.value"
        :user="workspace.user.value"
      />
    </main>
  </div>
</template>
