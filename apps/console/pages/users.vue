<script setup lang="ts">
useHead({ title: "App users · OWLEYE" });
</script>

<template>
  <ConsoleSectionShell
    description="Add users and manage their access to this app."
    eyebrow="Access control"
    :permission="null"
    :allow-empty="true"
    title="Users"
    v-slot="{ demo, site, user }"
  >
    <InstanceUsers v-if="user?.is_admin" />
    <MemberManager
      v-if="site?.permissions?.users_manage"
      :demo="demo"
      :site-id="site?.id"
    />
    <p v-if="!user?.is_admin && !site?.permissions?.users_manage">
      Administrator access is required.
    </p>
  </ConsoleSectionShell>
</template>
