<script setup lang="ts">
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";
import { browserTimezone } from "~/utils/timezones";
import { trackingSetupSnippet } from "~/utils/trackingSnippet";

type OnboardingStep = "create_site" | "two_factor";

type CreatedSite = {
  id: string;
  name: string;
  public_key?: string | null;
  tracking_id: string;
};

const props = withDefaults(
  defineProps<{
    apiBase: string;
    email: string;
    signOutError?: string;
    signOutPending?: boolean;
    step: OnboardingStep;
  }>(),
  { signOutError: "", signOutPending: false },
);

const emit = defineEmits<{
  changed: [];
  finished: [trackingId: string];
  "sign-out": [];
}>();
const { api } = useApi();

const setupSecret = ref("");
const setupUri = ref("");
const authenticatorCode = ref("");
const appName = ref("");
const domains = ref("");
const timezone = ref("UTC");
onMounted(() => {
  timezone.value = browserTimezone();
});
const createdSite = ref<CreatedSite | null>(null);
const pending = ref(false);
const error = ref("");
const copyState = ref<"copied" | "failed" | "idle">("idle");

const trackingSnippet = computed(() => {
  if (!createdSite.value) return "";
  return trackingSetupSnippet(createdSite.value.tracking_id, props.apiBase);
});

watch(
  () => props.step,
  () => {
    error.value = "";
  },
);

async function beginAuthenticatorSetup() {
  await run(async () => {
    const response = await api<{
      otpauth_url: string;
      secret: string;
    }>(routes.auth.twoFactorSetup, {
      method: "POST",
    });
    setupSecret.value = response.secret;
    setupUri.value = response.otpauth_url;
  }, "Authenticator setup could not be started.");
}

async function verifyAuthenticator() {
  await run(async () => {
    await api(routes.auth.twoFactorEnable, {
      body: { code: authenticatorCode.value, secret: setupSecret.value },
      method: "POST",
    });
    emit("changed");
  }, "That authenticator code did not match. Try the newest code.");
}

async function skipAuthenticator() {
  await run(async () => {
    await api(routes.auth.onboardingSkipTwoFactor, {
      method: "POST",
    });
    emit("changed");
  }, "The 2FA prompt could not be skipped. Try again.");
}

async function createFirstApp() {
  await run(async () => {
    createdSite.value = await api<CreatedSite>(routes.sites.list, {
      body: {
        domains: domains.value,
        timezone: timezone.value,
        name: appName.value,
      },
      method: "POST",
    });
  }, "Your app could not be created. Check the name and domains.");
}

async function copySnippet() {
  try {
    await navigator.clipboard.writeText(trackingSnippet.value);
    copyState.value = "copied";
  } catch {
    copyState.value = "failed";
  }
}

async function run(action: () => Promise<void>, fallback: string) {
  if (pending.value) return;
  error.value = "";
  pending.value = true;
  try {
    await action();
  } catch (cause) {
    error.value = requestError(cause, fallback);
  } finally {
    pending.value = false;
  }
}
</script>

<template>
  <main id="main-content" class="onboarding-shell">
    <section class="onboarding-panel" aria-labelledby="onboarding-heading">
      <header class="onboarding-header">
        <BrandLockup />
        <div class="onboarding-header-actions">
          <div class="onboarding-progress" aria-label="Onboarding progress">
            <span :class="{ active: step === 'two_factor' }"
              >01 · Security</span
            >
            <span :class="{ active: step === 'create_site' }"
              >02 · First app</span
            >
          </div>
          <button
            class="button secondary onboarding-signout"
            :disabled="signOutPending || pending"
            type="button"
            @click="emit('sign-out')"
          >
            {{ signOutPending ? "Signing out…" : "Switch account" }}
          </button>
        </div>
      </header>

      <p
        v-if="signOutError"
        class="auth-error onboarding-signout-error"
        role="alert"
      >
        {{ signOutError }}
      </p>

      <div v-if="step === 'two_factor'" class="onboarding-content">
        <div class="onboarding-copy">
          <p class="eyebrow">Optional, but sensible</p>
          <h1 id="onboarding-heading">Give your dashboard a second lock.</h1>
          <p>
            Use any authenticator app that supports TOTP. No SMS, no email codes
            pretending to be a second factor, no security theatre.
          </p>
          <small>Signed in as {{ email }}</small>
        </div>

        <div class="onboarding-action-card">
          <template v-if="!setupSecret">
            <h2>Authenticator app</h2>
            <p>
              Scan a QR code with 1Password, Google Authenticator, Authy, or
              another TOTP app. You can also enter a setup key manually.
            </p>
            <div class="onboarding-actions">
              <button
                class="button primary"
                :disabled="pending"
                type="button"
                @click="beginAuthenticatorSetup"
              >
                {{ pending ? "Preparing…" : "Set up authenticator" }}
              </button>
              <button
                class="button secondary"
                :disabled="pending"
                type="button"
                @click="skipAuthenticator"
              >
                Skip for now
              </button>
            </div>
          </template>

          <form
            v-else
            class="onboarding-form"
            @submit.prevent="verifyAuthenticator"
          >
            <AuthenticatorSetup :secret="setupSecret" :uri="setupUri" />
            <label>
              <span>Six-digit authenticator code</span>
              <input
                v-model="authenticatorCode"
                autocomplete="one-time-code"
                inputmode="numeric"
                maxlength="6"
                pattern="[0-9]{6}"
                placeholder="123456"
                required
              />
            </label>
            <button class="button primary" :disabled="pending" type="submit">
              {{ pending ? "Checking…" : "Enable 2FA" }}
            </button>
          </form>
        </div>
      </div>

      <div v-else-if="!createdSite" class="onboarding-content">
        <div class="onboarding-copy">
          <p class="eyebrow">One app, coming up</p>
          <h1 id="onboarding-heading">Name it. Domains can wait.</h1>
          <p>
            Add production domains now or leave them blank. Localhost tracking
            is available when enabled in server setup.
          </p>
        </div>

        <form
          class="onboarding-action-card onboarding-form"
          @submit.prevent="createFirstApp"
        >
          <label>
            <span>App name</span>
            <input
              v-model="appName"
              autocomplete="off"
              maxlength="120"
              placeholder="Acme website"
              required
            />
          </label>
          <label>
            <span>Allowed domains <small>optional</small></span>
            <textarea
              v-model="domains"
              autocomplete="off"
              placeholder="acme.com, app.acme.com"
              rows="3"
              spellcheck="false"
            ></textarea>
          </label>
          <p class="field-help">
            Leave blank to allow all browser origins, or enter comma-separated
            URLs or hostnames to restrict tracking to those domains. Paths and
            query strings are not accepted.
          </p>
          <TimezoneSelect v-model="timezone" />
          <button class="button primary" :disabled="pending" type="submit">
            {{ pending ? "Creating…" : "Create my app" }}
          </button>
        </form>
      </div>

      <div v-else class="onboarding-complete">
        <div class="onboarding-copy">
          <p class="eyebrow">App created</p>
          <h1 id="onboarding-heading">
            {{ createdSite.name }} is ready to observe.
          </h1>
          <p>
            This is real configuration from the API, not a browser-made demo.
            Keep the tracking ID in your client setup.
          </p>
          <div class="created-identifiers">
            <span>Tracking ID</span>
            <code>{{ createdSite.tracking_id }}</code>
            <template v-if="createdSite.public_key">
              <span>Public key</span>
              <code>{{ createdSite.public_key }}</code>
            </template>
          </div>
          <button
            class="button primary"
            type="button"
            @click="emit('finished', createdSite.tracking_id)"
          >
            Open dashboard ↗
          </button>
        </div>

        <div class="onboarding-snippet">
          <div class="snippet-heading">
            <span>Client entry</span>
            <button type="button" @click="copySnippet">
              {{ copyState === "copied" ? "Copied ✓" : "Copy code" }}
            </button>
          </div>
          <pre><code>{{ trackingSnippet }}</code></pre>
          <p class="tracking-help">
            Custom events accept up to 10 string, number, or boolean properties,
            including global fields.
            <a
              href="https://github.com/shrinathprabhu/owleye/blob/HEAD/packages/analytics/README.md#cdn-campaigns-custom-data-rules-and-performance"
              target="_blank"
              rel="noopener noreferrer"
              >CDN setup, manual events, campaigns, rules, and performance ↗</a
            >
          </p>
          <TrackingAssistantPrompt
            :site-id="createdSite.tracking_id"
            :api-base="apiBase"
          />
          <p v-if="copyState === 'failed'" role="status">
            Clipboard access was blocked. Select and copy the snippet manually.
          </p>
        </div>
      </div>

      <p v-if="error" class="auth-error onboarding-error" role="alert">
        {{ error }}
      </p>
    </section>
  </main>
</template>

<style scoped>
.tracking-help {
  font-size: 0.78rem;
  line-height: 1.5;
}
.tracking-help a {
  color: inherit;
  text-decoration: underline;
}

.onboarding-shell {
  display: grid;
  min-height: 100dvh;
  place-items: center;
  background:
    radial-gradient(
      circle at 7% 16%,
      rgb(209 255 61 / 0.28),
      transparent 23rem
    ),
    radial-gradient(circle at 93% 83%, rgb(61 90 254 / 0.2), transparent 26rem),
    var(--bg-base);
  padding: clamp(16px, 4vw, 56px);
}

.onboarding-panel {
  width: min(1040px, 100%);
  border: 2px solid var(--owleye-ink);
  border-radius: clamp(22px, 3vw, 34px);
  background: var(--bg-elevated);
  padding: clamp(18px, 3vw, 32px);
  box-shadow: 9px 9px 0 var(--owleye-ink);
}

.onboarding-header,
.onboarding-header-actions,
.onboarding-progress,
.snippet-heading,
.created-identifiers {
  display: flex;
  align-items: center;
}

.onboarding-header {
  justify-content: space-between;
  gap: 18px;
  padding-bottom: clamp(20px, 3vw, 32px);
}

.onboarding-header-actions {
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 10px;
}

.onboarding-signout {
  min-height: 36px;
  padding: 7px 12px;
  font-size: 0.75rem;
}

.onboarding-progress {
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}

.onboarding-progress span {
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  color: var(--text-tertiary);
  padding: 7px 11px;
  font-family: var(--font-mono);
  font-size: 0.68rem;
  font-weight: 760;
  text-transform: uppercase;
}

.onboarding-progress span.active {
  border-color: var(--owleye-ink);
  color: var(--owleye-ink);
  background: var(--accent);
}

.onboarding-content,
.onboarding-complete {
  display: grid;
  grid-template-columns: minmax(0, 0.9fr) minmax(340px, 1.1fr);
  gap: clamp(24px, 4vw, 48px);
  align-items: center;
}

.onboarding-copy h1 {
  max-width: 18ch;
  margin: 8px 0 16px;
  font-size: clamp(1.8rem, 3.5vw, 2.75rem);
  letter-spacing: -0.045em;
  line-height: 1.12;
}

.onboarding-copy > p:not(.eyebrow) {
  max-width: 50ch;
  color: var(--text-secondary);
  line-height: 1.65;
}

.onboarding-copy > small {
  display: block;
  margin-top: 18px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
}

.onboarding-action-card,
.onboarding-snippet {
  min-width: 0;
  border: 2px solid var(--owleye-ink);
  border-radius: var(--radius-xl);
  background: var(--bg-subtle);
  padding: clamp(18px, 2.5vw, 26px);
  box-shadow: 6px 6px 0 var(--brand);
}

.onboarding-action-card > h2 {
  margin: 0 0 8px;
  font-size: clamp(1.25rem, 2vw, 1.6rem);
}

.onboarding-action-card > p {
  color: var(--text-secondary);
  line-height: 1.55;
}

.onboarding-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 20px;
}

.onboarding-form {
  display: grid;
  gap: 16px;
}

.onboarding-form label {
  display: grid;
  gap: 7px;
  font-weight: 760;
}

.onboarding-form label small {
  color: var(--text-tertiary);
  font-weight: 600;
}

.onboarding-form input,
.onboarding-form textarea {
  width: 100%;
  border: 1.5px solid var(--border-strong);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  background: var(--bg-elevated);
  padding: 13px 14px;
  font: inherit;
}

.onboarding-form textarea {
  resize: vertical;
}

.field-help {
  margin: -6px 0 0;
  color: var(--text-tertiary);
  font-size: 0.78rem;
  line-height: 1.45;
}

.created-identifiers {
  align-items: flex-start;
  flex-direction: column;
  gap: 5px;
  margin: 20px 0;
}

.created-identifiers span {
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  font-size: 0.67rem;
  font-weight: 800;
  text-transform: uppercase;
}

.created-identifiers code {
  max-width: 100%;
  overflow-wrap: anywhere;
  margin-bottom: 9px;
  font-family: var(--font-mono);
  font-size: 0.9rem;
}

.onboarding-snippet {
  color: #f7f7f2;
  background: #111214;
  box-shadow: 6px 6px 0 var(--accent);
}

.snippet-heading {
  justify-content: space-between;
  gap: 14px;
  color: #a8a9ad;
  font-family: var(--font-mono);
  font-size: 0.7rem;
  font-weight: 800;
  text-transform: uppercase;
}

.snippet-heading button {
  border: 0;
  color: var(--accent);
  background: transparent;
  font: inherit;
  cursor: pointer;
}

.onboarding-snippet pre {
  max-width: 100%;
  overflow: auto;
  margin: 16px 0 0;
  font-family: var(--font-mono);
  font-size: 0.76rem;
  line-height: 1.65;
}

.onboarding-error {
  margin: 24px 0 0;
}

.onboarding-signout-error {
  margin: -18px 0 24px;
}

@media (max-width: 760px) {
  .onboarding-shell {
    place-items: start stretch;
    padding: 10px;
  }

  .onboarding-panel {
    box-shadow: 5px 5px 0 var(--owleye-ink);
  }

  .onboarding-header {
    align-items: flex-start;
    flex-direction: column;
  }

  .onboarding-header-actions {
    width: 100%;
    align-items: flex-start;
    flex-direction: column;
  }

  .onboarding-progress {
    justify-content: flex-start;
  }

  .onboarding-content,
  .onboarding-complete {
    grid-template-columns: minmax(0, 1fr);
  }

  .onboarding-copy h1 {
    font-size: clamp(1.8rem, 6vw, 2.5rem);
  }
}

@media (max-width: 480px) {
  .onboarding-actions .button {
    width: 100%;
  }
}

@media (max-width: 400px) {
  .onboarding-panel {
    padding: 16px 14px 22px;
  }

  .onboarding-progress span {
    padding-inline: 8px;
    font-size: 0.61rem;
  }

  .onboarding-action-card,
  .onboarding-snippet {
    padding: 16px 14px;
  }
}
</style>
