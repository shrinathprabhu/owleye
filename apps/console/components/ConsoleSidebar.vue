<script setup lang="ts">
import type { ConsoleRole, ConsoleUserSummary } from "~/types/console";
import type { WorkspacePermissions, WorkspaceSite } from "~/types/workspace";

const props = withDefaults(
  defineProps<{
    collapsed?: boolean;
    currentRole?: ConsoleRole;
    demo?: boolean;
    permissions?: Partial<WorkspacePermissions>;
    entitlements?: WorkspaceSite["entitlements"];
    siteId?: string;
    signOutPending?: boolean;
    user: ConsoleUserSummary;
  }>(),
  {
    collapsed: false,
    currentRole: "viewer",
    demo: false,
    permissions: () => ({}),
    siteId: "",
    signOutPending: false,
  },
);

const emit = defineEmits<{
  "sign-out": [];
  "update:collapsed": [collapsed: boolean];
}>();

type NavIconName =
  | "overview"
  | "pro-view"
  | "ai"
  | "rules"
  | "api-keys"
  | "events"
  | "funnels"
  | "campaigns"
  | "performance"
  | "users"
  | "settings";

type NavItem = {
  icon: NavIconName;
  label: string;
  to: string;
};

const route = useRoute();
const mobileOpen = ref(false);
const isMobile = ref(false);
const mobileTrigger = ref<HTMLButtonElement | null>(null);
const sidebarClose = ref<HTMLButtonElement | null>(null);
const sidebarRoot = ref<HTMLElement | null>(null);
let mediaQuery: MediaQueryList | undefined;

const initials = computed(() => {
  const source = props.user.name?.trim() || props.user.email;
  return source
    .split(/[\s@._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join("");
});

const roleLabel = computed(() => {
  if (props.currentRole === "owner") return "Owner";
  if (props.currentRole === "admin") return "Admin";
  return "Read only";
});

const siteQuery = computed(() => ({
  site: props.siteId || undefined,
}));
const overviewHref = computed(() =>
  props.siteId ? `/?site=${encodeURIComponent(props.siteId)}` : "/",
);

const navItems = computed<NavItem[]>(() => {
  const items: NavItem[] = [
    { icon: "overview", label: "Overview", to: "/" },
    { icon: "events", label: "Live", to: "/live" },
    { icon: "pro-view", label: "Pro View", to: "/pro-view" },
    { icon: "funnels", label: "Funnels", to: "/funnels" },
    { icon: "campaigns", label: "Campaigns", to: "/campaigns" },
    { icon: "performance", label: "Web Vitals", to: "/performance" },
  ];

  if (!props.demo) {
    items.push({ icon: "performance", label: "Uptime", to: "/uptime" });
  }

  if (props.permissions.ai_use) {
    items.push({ icon: "ai", label: "AI", to: "/ai" });
  }

  if (props.permissions.rules_write) {
    items.push({ icon: "rules", label: "Rules", to: "/rules" });
    items.push({ icon: "api-keys", label: "API Keys", to: "/api-keys" });
  }

  items.push({ icon: "events", label: "Events", to: "/events" });

  if (props.permissions.users_manage || props.user.is_admin) {
    items.push({ icon: "users", label: "Users", to: "/users" });
  }
  if (props.permissions.settings_manage) {
    items.push({ icon: "settings", label: "Settings", to: "/settings" });
  }

  return items;
});

watch(
  () => route.fullPath,
  () => closeMobile(),
);

watch(mobileOpen, (open) => {
  if (!import.meta.client) return;
  document.body.classList.toggle("console-drawer-open", open && isMobile.value);
  if (open) nextTick(() => sidebarClose.value?.focus());
});

onMounted(() => {
  mediaQuery = window.matchMedia("(max-width: 1100px)");
  syncViewport(mediaQuery);
  mediaQuery.addEventListener("change", syncViewport);
  window.addEventListener("keydown", handleWindowKeydown);
});

onBeforeUnmount(() => {
  mediaQuery?.removeEventListener("change", syncViewport);
  window.removeEventListener("keydown", handleWindowKeydown);
  document.body.classList.remove("console-drawer-open");
});

function syncViewport(event: MediaQueryList | MediaQueryListEvent) {
  isMobile.value = event.matches;
  if (!event.matches) mobileOpen.value = false;
}

function handleWindowKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && mobileOpen.value) {
    event.preventDefault();
    closeMobile({ restoreFocus: true });
  }
}

function trapMobileFocus(event: KeyboardEvent) {
  if (!isMobile.value || !mobileOpen.value || event.key !== "Tab") return;
  const focusable = Array.from(
    sidebarRoot.value?.querySelectorAll<HTMLElement>(
      'a[href], button:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ) ?? [],
  ).filter((element) => !element.hasAttribute("inert"));
  const first = focusable[0];
  const last = focusable.at(-1);
  if (!first || !last) return;

  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

function openMobile() {
  mobileOpen.value = true;
}

function closeMobile({ restoreFocus = false } = {}) {
  if (!mobileOpen.value) return;
  mobileOpen.value = false;
  if (restoreFocus) nextTick(() => mobileTrigger.value?.focus());
}

function toggleCollapsed() {
  emit("update:collapsed", !props.collapsed);
}

function isActive(path: string) {
  return path === "/" ? route.path === "/" : route.path.startsWith(path);
}
</script>

<template>
  <button
    ref="mobileTrigger"
    class="mobile-menu-button"
    type="button"
    aria-controls="console-sidebar"
    :aria-expanded="mobileOpen"
    @click="openMobile"
  >
    <ConsoleNavIcon name="menu" />
    <span>Menu</span>
  </button>

  <button
    v-if="mobileOpen"
    class="sidebar-backdrop"
    type="button"
    tabindex="-1"
    aria-label="Close navigation"
    @click="closeMobile({ restoreFocus: true })"
  ></button>

  <aside
    id="console-sidebar"
    ref="sidebarRoot"
    class="sidebar"
    :class="{ collapsed, 'mobile-open': mobileOpen }"
    :aria-hidden="isMobile && !mobileOpen ? 'true' : undefined"
    :inert="isMobile && !mobileOpen"
    @keydown="trapMobileFocus"
  >
    <div class="sidebar-heading">
      <BrandLockup :href="overviewHref" />
      <button
        ref="sidebarClose"
        class="sidebar-collapse-button"
        type="button"
        :aria-label="
          isMobile
            ? 'Close navigation'
            : collapsed
              ? 'Expand navigation'
              : 'Collapse navigation'
        "
        :title="
          isMobile
            ? 'Close navigation'
            : collapsed
              ? 'Expand sidebar'
              : 'Collapse sidebar'
        "
        @click="
          isMobile ? closeMobile({ restoreFocus: true }) : toggleCollapsed()
        "
      >
        <ConsoleNavIcon :name="isMobile ? 'close' : 'collapse'" />
      </button>
    </div>

    <nav class="nav-stack" aria-label="Console sections">
      <NuxtLink
        v-for="item in navItems"
        :key="item.to"
        :to="{ path: item.to, query: siteQuery }"
        :class="{ active: isActive(item.to) }"
        :aria-current="isActive(item.to) ? 'page' : undefined"
        :title="collapsed && !isMobile ? item.label : undefined"
      >
        <ConsoleNavIcon :name="item.icon" />
        <span class="nav-label">{{ item.label }}</span>
      </NuxtLink>
    </nav>

    <div class="sidebar-bottom">
      <nav class="nav-stack" aria-label="Account">
        <NuxtLink
          to="/profile"
          :class="{ active: isActive('/profile') }"
          :aria-current="isActive('/profile') ? 'page' : undefined"
          :title="collapsed && !isMobile ? 'Profile' : undefined"
        >
          <ConsoleNavIcon name="profile" />
          <span class="nav-label">Profile</span>
        </NuxtLink>
      </nav>
      <p class="sidebar-note">
        <template v-if="demo">
          Privacy-first analytics.<br />API-backed results.<br />Read-only
          actions stay read-only.
        </template>
        <template v-else>
          Cookie-free analytics<br />on infrastructure you control.
        </template>
      </p>

      <a
        class="sidebar-source"
        href="https://github.com/shrinathprabhu/owleye"
        rel="noopener noreferrer"
        target="_blank"
        title="View OwlEye source code"
      >
        <ConsoleNavIcon name="source" />
        <span>Source</span>
      </a>

      <div class="sidebar-account" :title="user.email">
        <div class="sidebar-avatar" aria-hidden="true">{{ initials }}</div>
        <div class="sidebar-account-copy">
          <span v-if="demo" class="demo-user-label">Read-only workspace</span>
          <strong>{{ user.name || user.email }}</strong>
          <span v-if="user.name">{{ user.email }}</span>
          <small v-if="route.meta.siteScoped !== false">{{ roleLabel }}</small>
        </div>
        <button
          class="sidebar-signout"
          type="button"
          :disabled="signOutPending"
          :aria-label="signOutPending ? 'Signing out' : 'Sign out'"
          :title="signOutPending ? 'Signing out…' : 'Sign out'"
          @click="emit('sign-out')"
        >
          <span
            v-if="signOutPending"
            class="sidebar-spinner"
            aria-hidden="true"
          ></span>
          <ConsoleNavIcon v-else name="sign-out" />
          <span class="sidebar-signout-label">
            {{ signOutPending ? "Signing out…" : "Sign out" }}
          </span>
        </button>
      </div>
    </div>
  </aside>
</template>
