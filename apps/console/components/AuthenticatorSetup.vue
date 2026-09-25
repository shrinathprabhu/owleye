<script setup lang="ts">
const props = defineProps<{ secret: string; uri: string }>();
const manual = ref(false);
const qrImage = ref("");
const qrFailed = ref(false);
const authenticatorUri = computed(() => {
  try {
    const url = new URL(props.uri);
    return url.protocol === "otpauth:" && url.hostname === "totp"
      ? props.uri
      : "";
  } catch {
    return "";
  }
});

watch(
  () => props.uri,
  async (_uri, _previous, onCleanup) => {
    manual.value = false;
    qrImage.value = "";
    qrFailed.value = false;
    let cancelled = false;
    onCleanup(() => {
      cancelled = true;
    });
    if (!import.meta.client) return;
    if (!authenticatorUri.value) {
      qrFailed.value = true;
      return;
    }
    try {
      const { default: QRCode } = await import("qrcode");
      const image = await QRCode.toDataURL(authenticatorUri.value, {
        errorCorrectionLevel: "M",
        margin: 4,
        width: 448,
        color: { dark: "#000000", light: "#ffffff" },
      });
      if (!cancelled) qrImage.value = image;
    } catch {
      if (!cancelled) qrFailed.value = true;
    }
  },
  { immediate: true },
);
</script>

<template>
  <div class="authenticator-setup">
    <template v-if="manual">
      <p class="panel-kicker">Manual setup key</p>
      <p class="setup-help">
        Enter this key in your authenticator app and choose time-based codes.
      </p>
      <code class="manual-key">{{ secret }}</code>
    </template>
    <template v-else>
      <p class="panel-kicker">Scan with your authenticator</p>
      <p class="setup-help">
        Open your authenticator app, add an account, and scan this QR code.
      </p>
      <div class="qr-frame">
        <img
          v-if="qrImage"
          :src="qrImage"
          width="224"
          height="224"
          alt="QR code for setting up OwlEye two-factor authentication"
        />
        <p v-else-if="qrFailed" role="status">
          QR code unavailable. Use the setup key below instead.
        </p>
        <p v-else role="status">Preparing QR code…</p>
      </div>
    </template>
    <div class="setup-options">
      <button class="button secondary" type="button" @click="manual = !manual">
        {{ manual ? "Show QR code" : "Use setup key instead" }}
      </button>
      <a
        v-if="authenticatorUri"
        class="authenticator-link"
        :href="authenticatorUri"
        >Open in an authenticator app ↗</a
      >
    </div>
  </div>
</template>

<style scoped>
.authenticator-setup {
  display: grid;
  gap: 12px;
  min-width: 0;
}
.authenticator-setup .panel-kicker,
.setup-help {
  margin: 0;
}
.setup-help {
  color: var(--text-secondary);
  font-size: 0.9rem;
  line-height: 1.5;
}
.qr-frame {
  display: grid;
  place-items: center;
  width: min(224px, 100%);
  aspect-ratio: 1;
  justify-self: center;
  background: #fff;
  color: #111;
  border-radius: 8px;
  overflow: hidden;
}
.qr-frame img {
  display: block;
  width: 100%;
  height: auto;
}
.qr-frame p {
  margin: 16px;
  text-align: center;
  font-size: 0.85rem;
}
.manual-key {
  display: block;
  overflow-wrap: anywhere;
  border-radius: var(--radius-sm);
  background: var(--accent);
  color: var(--owleye-ink);
  padding: 12px;
  font-family: var(--font-mono);
  font-weight: 800;
  user-select: all;
}
.setup-options {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
}
.setup-options .button {
  font-size: 0.85rem;
}
.authenticator-link {
  color: var(--brand-text-safe);
  font-size: 0.85rem;
  font-weight: 760;
}
</style>
