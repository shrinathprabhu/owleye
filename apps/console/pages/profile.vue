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
    <BentoGrid v-if="user && !demo" class="profile-bento">
      <ReleaseNotice v-if="user.is_admin" />
      <AccountProfile @updated="updateName" />
      <AccountSecurity
        :api-base="apiBase"
        :user="user"
        @changed="workspace.initialize"
      />
    </BentoGrid>
    <p v-else class="empty-state">
      Profile changes are unavailable in this read-only workspace.
    </p>
  </ConsoleSectionShell>
</template>

<style scoped>
.profile-bento :deep(.profile-details-grid),
.profile-bento :deep(.account-security-grid) {
  display: contents;
}
</style>
