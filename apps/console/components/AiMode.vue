<script setup lang="ts">
import type { AiModeResponse, AiPromptResponse } from "~/types/ai";
import { apiErrorMessage as requestError } from "~/utils/apiError";
import { routes } from "~/utils/routes";

const props = defineProps<{
  siteId?: string;
  siteTrackingId?: string;
}>();
const { api } = useApi();

type SubmittedPrompt = {
  id: string;
  text: string;
  reportQuestion?: string;
  state: "pending" | "complete" | "clarification" | "error";
  response?: AiPromptResponse;
  error?: string;
};

const status = ref<AiModeResponse | null>(null);
const prompt = ref("");
const clarificationContext = ref<
  Array<{ role: "user" | "assistant"; text: string }>
>([]);
function newQuestion() {
  clarificationContext.value = [];
  composer.value?.focus();
}
const submitted = ref<SubmittedPrompt[]>([]);
const loading = ref(false);
const sending = ref(false);
const error = ref("");
const conversation = ref<HTMLElement | null>(null);
const composer = ref<HTMLTextAreaElement | null>(null);
const followingLatest = ref(true);
let requestController: AbortController | undefined;
let promptController: AbortController | undefined;

watch(
  () => props.siteId,
  () => {
    requestController?.abort();
    promptController?.abort();
    status.value = null;
    submitted.value = [];
    clarificationContext.value = [];
    prompt.value = "";
    followingLatest.value = true;
    error.value = "";
    sending.value = false;
    void loadStatus();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  requestController?.abort();
  promptController?.abort();
});

async function loadStatus(quiet = false) {
  const siteId = props.siteId;
  if (!siteId) {
    status.value = null;
    loading.value = false;
    return;
  }
  requestController?.abort();
  const controller = new AbortController();
  requestController = controller;
  if (!quiet) {
    loading.value = true;
    error.value = "";
  }
  try {
    const response = await api<AiModeResponse>(routes.sites.ai(siteId), {
      signal: controller.signal,
    });
    if (controller.signal.aborted || props.siteId !== siteId) return;
    status.value = response;
  } catch (cause) {
    if (controller.signal.aborted || props.siteId !== siteId) return;
    if (!quiet) {
      status.value = null;
      error.value = requestError(
        cause,
        "I couldn’t connect to your analytics assistant. Please try again.",
      );
    }
  } finally {
    if (requestController === controller) {
      requestController = undefined;
      loading.value = false;
    }
  }
}

function trackScroll() {
  const el = conversation.value;
  if (!el) return;
  const last = el.lastElementChild?.getBoundingClientRect();
  const viewport = el.getBoundingClientRect();
  followingLatest.value =
    el.scrollHeight - el.scrollTop - el.clientHeight < 80 ||
    !!(last && last.top >= viewport.top && last.top < viewport.bottom);
}
async function revealLatest(revealPage = false) {
  await nextTick();
  const el = conversation.value;
  if (!el) return;
  const exchange = el.lastElementChild as HTMLElement | null;
  const behavior = window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ? "instant"
    : "smooth";
  if (revealPage) el.scrollIntoView({ block: "nearest", behavior });
  if (exchange)
    el.scrollTo({
      top:
        exchange.getBoundingClientRect().top -
        el.getBoundingClientRect().top +
        el.scrollTop,
      behavior,
    });
}
function editQuestion(text: string) {
  prompt.value = text;
  composer.value?.focus();
}
function onComposerKeydown(event: KeyboardEvent) {
  if (
    event.key === "Enter" &&
    (event.metaKey || event.ctrlKey) &&
    !event.isComposing
  ) {
    event.preventDefault();
    void sendPrompt();
  }
}
async function sendPrompt() {
  const text = prompt.value.trim();
  const siteId = props.siteId;
  if (!siteId || !status.value?.can_use || !text || sending.value) return;
  promptController?.abort();
  requestController?.abort();
  const controller = new AbortController();
  promptController = controller;
  const entry = reactive<SubmittedPrompt>({
    id: crypto.randomUUID(),
    text,
    reportQuestion: [
      ...clarificationContext.value
        .filter((turn) => turn.role === "user")
        .map((turn) => turn.text),
      text,
    ].join("\n"),
    state: "pending",
  });
  submitted.value.push(entry);
  prompt.value = "";
  sending.value = true;
  error.value = "";
  followingLatest.value = true;
  void revealLatest(true);
  try {
    const response = await api<AiPromptResponse>(routes.sites.prompt(siteId), {
      body: {
        prompt: text,
        request_id: entry.id,
        context: clarificationContext.value,
      },
      retry: 0,
      method: "POST",
      signal: controller.signal,
    });
    if (controller.signal.aborted || props.siteId !== siteId) return;
    entry.response = response;
    entry.state = response.accepted ? "complete" : "clarification";
    if (response.accepted) clarificationContext.value = [];
    else {
      clarificationContext.value = [
        ...clarificationContext.value,
        { role: "user", text },
        { role: "assistant", text: response.answer },
      ];
      // Keep the original question and the latest clarification exchanges.
      if (clarificationContext.value.length > 8)
        clarificationContext.value = [
          ...clarificationContext.value.slice(0, 2),
          ...clarificationContext.value.slice(-6),
        ];
    }
  } catch (cause) {
    if (controller.signal.aborted || props.siteId !== siteId) return;
    entry.state = "error";
    entry.error = requestError(
      cause,
      "I couldn’t finish that request. Please try again in a moment.",
    );
    // Refresh availability without replacing the conversation or hiding the reply.
    void loadStatus(true);
  } finally {
    if (promptController === controller) {
      promptController = undefined;
      sending.value = false;
      if (followingLatest.value) void revealLatest();
    }
  }
}
</script>

<template>
  <section class="ai-mode-layout">
    <header v-if="status" class="ai-budget-strip"><div><span>AI usage</span><strong>Unlimited</strong></div></header>

    <article
      v-if="status && !status.provider_available && !submitted.length"
      class="permission-panel"
    >
      <span class="permission-symbol" aria-hidden="true">✦</span>
      <div>
        <p class="eyebrow">AI is not available yet</p>
        <h2>The analytics assistant is awaiting activation.</h2>
        <p>
          Ask your server administrator to configure an AI provider.
        </p>
      </div>
    </article>
    <article
      v-else-if="status && !status.enabled && !submitted.length"
      class="permission-panel"
    >
      <span class="permission-symbol" aria-hidden="true">✦</span>
      <div>
        <p class="eyebrow">AI mode is off</p>
        <h2>The owner has not enabled AI for this app.</h2>
        <p>Enable AI in app settings to start asking questions.</p>
      </div>
    </article>
    <article
      v-else-if="status && !status.can_use && !submitted.length"
      class="permission-panel"
    >
      <span class="permission-symbol" aria-hidden="true">0</span>
      <div>
        <p class="eyebrow">AI unavailable</p>
        <h2>Your account does not currently have AI access.</h2>
        <p>
          Ask the app owner to enable AI in settings.
        </p>
      </div>
    </article>

    <article
      v-else
      class="analyst-chat"
      aria-label="OwlEye analytics conversation"
    >
      <header class="chat-header">
        <div class="chat-identity">
          <span class="chat-orb" aria-hidden="true"></span>
          <div>
            <p class="chat-eyebrow">OWLEYE ANALYST</p>
            <h2>A little clarity, on demand.</h2>
          </div>
        </div>
        <span class="chat-status" :class="{ busy: sending || loading }"
          ><i aria-hidden="true"></i
          >{{
            sending
              ? "Thinking"
              : loading
                ? "Connecting"
                : error
                  ? "Offline"
                  : "Ready"
          }}</span
        >
      </header>

      <div
        ref="conversation"
        class="chat-log"
        role="log"
        aria-label="Messages"
        aria-live="polite"
        aria-relevant="additions text"
        @scroll="trackScroll"
      >
        <div v-if="loading && !status" class="chat-loading" role="status">
          <span class="chat-orb thinking" aria-hidden="true"></span>Connecting
          to your analytics…
        </div>
        <div v-else-if="error && !status" class="chat-reply connection-error">
          <div class="reply-author">
            <span class="reply-avatar" aria-hidden="true">✦</span
            ><strong>OwlEye</strong>
          </div>
          <p>{{ error }}</p>
          <button class="chat-text-button" type="button" @click="loadStatus()">
            Try connecting again ↗
          </button>
        </div>
        <div v-else-if="!submitted.length" class="chat-welcome">
          <span class="welcome-mark" aria-hidden="true">✦</span>
          <h3>Your numbers have a story.</h3>
          <p>
            Ask about locations, events, traffic, or campaign conversions.<br />Include
            the filters and date range you want to explore.
          </p>
        </div>
        <article
          v-for="item in submitted"
          :key="item.id"
          class="chat-exchange"
          :data-state="item.state"
        >
          <div class="chat-sent">
            <span class="message-label">You</span>
            <p>{{ item.text }}</p>
          </div>
          <div
            class="chat-reply"
            :class="{ 'reply-error': item.state === 'error' }"
          >
            <div class="reply-author">
              <span
                class="reply-avatar"
                :class="{ thinking: item.state === 'pending' }"
                aria-hidden="true"
                >✦</span
              ><strong>OwlEye</strong
              ><span v-if="item.state === 'pending'" class="reply-label"
                >Thinking</span
              >
            </div>
            <div
              v-if="item.state === 'pending'"
              class="reply-pending"
              role="status"
              aria-label="OwlEye is thinking"
            >
              <span class="thinking-dots" aria-hidden="true"
                ><i></i><i></i><i></i></span
              ><span>Taking a look at your question…</span>
            </div>
            <template v-else-if="item.state === 'error'">
              <p class="reply-error-text">{{ item.error }}</p>
              <button
                class="chat-text-button"
                type="button"
                :disabled="sending"
                @click="editQuestion(item.text)"
              >
                Edit question ↗
              </button>
            </template>
            <div
              v-else-if="item.state === 'clarification'"
              class="clarification-reply"
            >
              <p>{{ item.response?.answer }}</p>
              <small>Reply below to continue.</small>
            </div>
            <AiReport
              v-else-if="item.response"
              :response="item.response"
              :question="item.reportQuestion ?? item.text"
            />
          </div>
        </article>
      </div>
      <button
        v-if="submitted.length && !followingLatest"
        class="latest-message"
        type="button"
        @click="revealLatest()"
      >
        Latest message ↓
      </button>

      <div v-if="clarificationContext.length" class="clarification-context">
        <span>Replying to the analyst’s clarification</span>
        <button
          type="button"
          class="chat-text-button"
          :disabled="sending"
          @click="newQuestion"
        >
          Start a new question
        </button>
      </div>
      <form class="chat-composer" @submit.prevent="sendPrompt">
        <label for="ai-prompt" class="visually-hidden">Your question</label>
        <textarea
          id="ai-prompt"
          ref="composer"
          v-model="prompt"
          aria-describedby="ai-privacy composer-help"
          :disabled="!status?.can_use"
          maxlength="4000"
          :placeholder="
            clarificationContext.length
              ? 'Your clarification…'
              : 'Ask about traffic, cities, events, or campaign conversions…'
          "
          required
          rows="2"
          @keydown="onComposerKeydown"
        ></textarea>
        <div class="composer-footer">
          <small id="composer-help"
            ><span class="keyboard-hint">⌘ / Ctrl + Enter to send</span
            ><span>{{ prompt.length.toLocaleString() }} / 4,000</span></small
          >
          <button
            class="chat-send"
            :disabled="sending || !prompt.trim() || !status?.can_use"
            type="submit"
            aria-label="Send prompt"
          >
            <span>{{ sending ? "Thinking…" : "Send" }}</span
            ><svg
              v-if="!sending"
              viewBox="0 0 24 24"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M12 19V5m-6 6 6-6 6 6"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              /></svg
            ><span v-else class="send-pulse" aria-hidden="true"></span>
          </button>
        </div>
      </form>
      <p id="ai-privacy" class="chat-privacy">
        Your question, clarification replies, aggregate results, and app catalog
        go to our AI provider. Avoid personal information or secrets. Check
        supporting data for accuracy. Clarifications keep context for the
        current question. This conversation isn’t saved to your account.
      </p>
    </article>
  </section>
</template>

<style scoped>
.clarification-context {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.75rem;
  color: #c8c9df;
  font-size: 0.85rem;
}
.clarification-reply {
  color: #eeeff7;
  line-height: 1.8;
  white-space: pre-wrap;
}
.clarification-reply small {
  color: #bfc1ca;
}

.analyst-chat {
  --chat-muted: #a1a5b8;
  background:
    radial-gradient(ellipse at 5% 0%, #30376455, transparent 45%), #101216;
  color: #eeeff7;
  border: 1px solid #343742;
  border-radius: 24px;
  padding: clamp(16px, 3vw, 32px);
  box-shadow: 0 12px 36px #11131b12;
  min-width: 0;
}
.chat-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding-bottom: 24px;
  border-bottom: 1px solid #ffffff0d;
}
.chat-identity {
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
}
.chat-eyebrow {
  margin: 0 0 4px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.15em;
  color: #b6baff;
}
.chat-header h2 {
  margin: 0;
  font-size: clamp(18px, 2vw, 24px);
  line-height: 1.3;
  letter-spacing: -0.035em;
}
.chat-orb {
  display: block;
  flex: none;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: radial-gradient(
    circle at 30% 25%,
    #f8ffde 0,
    #d9ff62 20%,
    #9e9aff 55%,
    #584df2 82%
  );
  box-shadow:
    inset -5px -5px 12px #27205d88,
    0 0 22px #9690ff30;
}
.chat-status {
  display: flex;
  align-items: center;
  gap: 7px;
  color: #bec4d0;
  font-size: 11px;
  white-space: nowrap;
}
.chat-status i {
  width: 6px;
  height: 6px;
  background: #c6ef79;
  border-radius: 50%;
}
.chat-status.busy i {
  background: #aaa3ff;
}
.chat-log {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 28px;
  min-height: 230px;
  max-height: min(620px, 65svh);
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-width: thin;
  scrollbar-color: #4d4e60 transparent;
  scrollbar-gutter: stable;
  padding: 26px 8px 24px 0;
  scroll-padding: 20px;
}
.chat-welcome {
  text-align: center;
  align-self: center;
  margin: auto;
  padding: 24px 12px;
  max-width: 450px;
}
.welcome-mark {
  display: block;
  color: #c9d9a5;
  font-size: 30px;
  margin-bottom: 12px;
}
.chat-welcome h3 {
  font-size: 21px;
  letter-spacing: -0.03em;
  margin: 0 0 10px;
}
.chat-welcome p {
  font-size: 13px;
  line-height: 1.8;
  color: var(--chat-muted);
  margin: 0;
}
.chat-exchange {
  display: flex;
  flex-direction: column;
  gap: 22px;
  flex: none;
  min-width: 0;
  overflow-wrap: anywhere;
}
.chat-sent {
  align-self: flex-end;
  max-width: min(82%, 680px);
  background: linear-gradient(135deg, #292c46, #242738);
  border: 1px solid #454968;
  border-radius: 19px 19px 5px 19px;
  padding: 14px 18px;
  color: #f0efff;
}
.message-label {
  display: block;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.03em;
  margin-bottom: 5px;
  color: #b7bbed;
}
.chat-sent p {
  margin: 0;
  line-height: 1.65;
  font-size: 14px;
  white-space: pre-wrap;
}
.chat-reply {
  align-self: stretch;
  min-width: 0;
  padding: 0 8px 0 0;
}
.reply-author {
  display: flex;
  align-items: center;
  gap: 9px;
  margin-bottom: 13px;
}
.reply-author strong {
  font-size: 12px;
  font-weight: 650;
  color: #d7d9e7;
}
.reply-avatar {
  display: grid;
  place-items: center;
  flex: none;
  width: 27px;
  height: 27px;
  border: 1px solid #aab28b45;
  background: #dae9aa14;
  border-radius: 9px;
  color: #d9eaa9;
  font-size: 17px;
}
.reply-label {
  color: var(--chat-muted);
  font-size: 11px;
  margin-left: 2px;
}
.reply-pending {
  display: flex;
  align-items: center;
  gap: 13px;
  min-height: 52px;
  border-radius: 14px;
  padding: 12px 15px;
  background: #ffffff04;
  color: #adb0c4;
  font-size: 13px;
}
.thinking-dots {
  display: flex;
  align-items: center;
  gap: 4px;
}
.thinking-dots i {
  height: 5px;
  width: 5px;
  border-radius: 50%;
  background: #b9b0ff;
  animation: thinking-dot 1.3s ease-in-out infinite;
}
.thinking-dots i:nth-child(2) {
  animation-delay: 0.16s;
}
.thinking-dots i:nth-child(3) {
  animation-delay: 0.32s;
}
.thinking {
  animation: orb-breathe 2.4s ease-in-out infinite;
}
.reply-error-text {
  margin: 0 0 10px;
  padding: 14px 17px;
  border: 1px solid #d9b27d26;
  border-radius: 4px 16px 16px;
  background: #d9b27d08;
  color: #e0d6c7;
  font-size: 14px;
  line-height: 1.75;
}
.chat-text-button {
  background: none;
  border: 0;
  color: #b9b6ee;
  font: inherit;
  font-size: 12px;
  padding: 6px 0;
  cursor: pointer;
}
.chat-text-button:disabled {
  opacity: 0.5;
  cursor: default;
}
.latest-message {
  display: block;
  margin: 0 auto 12px;
  border: 1px solid #4a4e64;
  background: #252838;
  border-radius: 20px;
  color: #e0e1ef;
  font-size: 12px;
  padding: 7px 14px;
  cursor: pointer;
}
.chat-composer {
  border: 1px solid #454858;
  background: #1b1d26;
  border-radius: 17px;
  padding: 14px 15px 12px;
  transition:
    border-color 0.15s,
    box-shadow 0.15s;
}
.chat-composer:focus-within {
  border-color: #9795f6;
  box-shadow: 0 0 0 3px #9995ff12;
}
.chat-composer textarea {
  display: block;
  width: 100%;
  min-height: 58px;
  max-height: 180px;
  resize: vertical;
  padding: 0;
  border: 0;
  background: transparent;
  box-shadow: none;
  outline: none;
  color: #f4f3ff;
  font: inherit;
  font-size: 14px;
  line-height: 1.7;
  border-radius: 0;
}
.chat-composer textarea::placeholder {
  color: #9397aa;
}
.chat-composer textarea:disabled {
  opacity: 0.5;
}
.composer-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  margin-top: 10px;
}
.composer-footer small {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  color: #a1a5b8;
  font-size: 10px;
}
.chat-send {
  display: flex;
  align-items: center;
  gap: 8px;
  border: 0;
  border-radius: 11px;
  padding: 9px 13px;
  color: #191a27;
  background: #d7ff72;
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}
.chat-send svg {
  width: 17px;
  height: 17px;
}
.chat-send:disabled {
  background: #343747;
  color: #a1a5ba;
  cursor: default;
}
.send-pulse {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: #b9b0ff;
  animation: orb-breathe 1.6s infinite;
}
.chat-privacy {
  margin: 12px 4px 0;
  font-size: 10px;
  line-height: 1.7;
  color: #9da1b2;
  max-width: 900px;
}
.chat-loading {
  display: flex;
  gap: 15px;
  align-items: center;
  margin: auto;
  color: #b7bac9;
  font-size: 13px;
}
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
button:focus-visible {
  outline: 2px solid #b9b0ff;
  outline-offset: 4px;
}
@keyframes thinking-dot {
  0%,
  60%,
  100% {
    transform: translateY(0);
    opacity: 0.45;
  }
  30% {
    transform: translateY(-4px);
    opacity: 1;
  }
}
@keyframes orb-breathe {
  0%,
  100% {
    opacity: 0.65;
  }
  50% {
    opacity: 1;
  }
}
@media (max-width: 600px) {
  .analyst-chat {
    border-radius: 18px;
    padding: 16px;
  }
  .chat-header {
    gap: 10px;
    padding-bottom: 18px;
  }
  .chat-identity {
    gap: 10px;
  }
  .chat-orb {
    width: 31px;
    height: 31px;
  }
  .chat-header h2 {
    font-size: 17px;
  }
  .chat-status {
    font-size: 10px;
  }
  .chat-log {
    padding-top: 20px;
    max-height: 60svh;
  }
  .chat-sent {
    max-width: 94%;
    padding: 12px 14px;
  }
  .chat-sent p {
    font-size: 13px;
  }
  .chat-composer {
    padding: 12px;
  }
  .keyboard-hint {
    display: none;
  }
  .reply-pending {
    font-size: 12px;
  }
  .chat-privacy {
    font-size: 10px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .thinking,
  .thinking-dots i,
  .send-pulse {
    animation: none;
  }
  .chat-composer {
    transition: none;
  }
}
</style>
