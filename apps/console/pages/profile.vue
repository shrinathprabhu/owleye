<script setup lang="ts">
definePageMeta({
  siteScoped: false,
  middleware: (to) => {
    if ("site" in to.query) {
      const { site: _site, ...query } = to.query;
      return navigateTo({ path: "/profile", query }, { replace: true });
    }
  },
});
useHead({ title: "Profile · OWLEYE" });
const workspace = useConsoleWorkspace();
function updateName(name: string) {
  const session = workspace.session.value;
  if (session?.authenticated) session.user.name = name;
}
</script>

<template>
  <ConsoleSectionShell
    title="Profile"
    eyebrow="Your account"
    description="Manage your personal profile and account security."
    :permission="null"
    :allow-empty="true"
    :show-site-selector="false"
    v-slot="{ apiBase, demo, user }"
  >
    <div v-if="user && !demo" class="profile-layout">
      <ReleaseNotice v-if="user.is_admin" />
      <AccountProfile @updated="updateName" />
      <AccountSecurity
        :api-base="apiBase"
        :user="user"
        @changed="workspace.initialize"
      />
    </div>
    <p v-else class="empty-state">
      Profile changes are unavailable in this read-only workspace.
    </p>
  </ConsoleSectionShell>
</template>

<style scoped>
/* Account forms keep natural grid rows: async errors and security sections
   must reserve their own height instead of sharing masonry measurements. */
.profile-layout {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: var(--space-4);
}
.profile-layout :deep(.profile-details-grid),
.profile-layout :deep(.account-security-grid) {
  display: contents;
}
.profile-layout :deep(.management-card) {
  min-width: 0;
  max-width: none;
  margin: 0;
}
@media (max-width: 1100px) {
  .profile-layout {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
