<script setup lang="ts">
import {
  breakdownItems,
  breakdownLabel,
  type BreakdownMetric,
} from "~/utils/breakdownMetric";
import { analyticsInterval, type AnalyticsRange } from "~/utils/analyticsRange";
import { apiErrorMessage as requestError } from "~/utils/apiError";

const config = useRuntimeConfig();
const { apiBase } = useApi();
const route = useRoute();
const router = useRouter();
const workspace = useConsoleWorkspace();
workspace.prepareForNavigation();

useHead({
  title: "Analytics overview · OWLEYE",
});

const privacyNoticeUrl = computed(() =>
  String(config.public.privacyNoticeUrl ?? "").trim(),
);
const termsUrl = computed(() => String(config.public.termsUrl ?? "").trim());
const authChallengeId = ref(
  typeof route.query.auth_challenge === "string"
    ? route.query.auth_challenge
    : "",
);
const authQueryError = ref(
  typeof route.query.auth_error === "string" ? route.query.auth_error : "",
);
const sidebarCollapsed = useState("console-sidebar-collapsed", () => false);
const rangeDays = ref(30);
const breakdownMetric = ref<BreakdownMetric>("pageviews");
const metricLabel = computed(() => breakdownLabel(breakdownMetric.value));
const audienceTotal = computed(() =>
  breakdownMetric.value === "visitors"
    ? stats.value?.totals.pageview_visitors
    : stats.value?.totals.pageviews,
);
const rangeDates = ref<AnalyticsRange["dates"]>();
const sessionPending = computed(
  () => workspace.loading.value && workspace.session.value === null,
);
const signOutPending = workspace.signOutPending;
const signOutError = workspace.error;
const sites = workspace.sites;
const sitesError = workspace.error;
const sitesPending = computed(
  () => workspace.loading.value && workspace.session.value?.authenticated,
);
const siteId = workspace.selectedSiteId;
const user = workspace.user;
const isAuthenticated = workspace.isAuthenticated;
const isDemoUser = workspace.isDemoUser;
const selectedSite = workspace.selectedSite;
const selectedRole = workspace.currentRole;
const selectedPermissions = workspace.permissions;
const siteOptions = workspace.siteOptions;
const normalizedSiteId = computed(() => siteId.value.trim());
const authError = computed(() => authQueryError.value || workspace.error.value);
const onboardingStep = computed<"create_site" | "two_factor" | null>(() => {
  const session = workspace.session.value;
  const nextStep = session?.authenticated
    ? session.onboarding.next_step
    : "complete";
  return nextStep === "complete" ? null : nextStep;
});
const {
  hasLoadedSite,
  refreshStats,
  resetStats,
  stats,
  statsError,
  statsPending,
} = useStatsOverview({
  days: rangeDays,
  dates: rangeDates,
  prefetchDemoRanges: isDemoUser,
  siteId: normalizedSiteId,
  toErrorMessage: requestError,
});
const canLoadStats = computed(
  () =>
    isAuthenticated.value &&
    normalizedSiteId.value.length > 0 &&
    !statsPending.value,
);
const loadedRangeDays = computed(() => stats.value?.days ?? rangeDays.value);
const rangeMetricCaption = computed(() =>
  statsPending.value && stats.value
    ? `Updating to ${rangeDays.value} days…`
    : stats.value
      ? `${stats.value.start_date} – ${stats.value.end_date} (UTC)`
      : `Selected ${loadedRangeDays.value}-day range`,
);

watch(siteId, () => {
  if (normalizedSiteId.value && isAuthenticated.value) void refreshStats();
  else resetStats();
});

onMounted(async () => {
  await clearAuthQueryParameters();
  await workspace.initialize({ force: false });
  if (isAuthenticated.value && normalizedSiteId.value) await refreshStats();
});

async function signOut() {
  resetStats();
  authChallengeId.value = "";
  authQueryError.value = "";
  await workspace.signOut();
}

async function refreshSession() {
  await workspace.initialize();
}

async function handleOnboardingChanged() {
  await refreshSession();
}

async function finishOnboarding(trackingId: string) {
  await workspace.initialize();
  await workspace.selectSite(trackingId);
  if (normalizedSiteId.value) await refreshStats();
}

async function clearAuthQueryParameters() {
  const authKeys = ["auth_challenge", "auth_error", "register", "tfa"];
  if (!authKeys.some((key) => key in route.query)) return;
  const query = { ...route.query };
  for (const key of authKeys) delete query[key];
  await router.replace({ query });
}

async function refreshSites() {
  await workspace.initialize();
}

async function handleAuthenticated() {
  workspace.resetIdentity();
  authChallengeId.value = "";
  authQueryError.value = "";
  await workspace.initialize();
  if (isAuthenticated.value && normalizedSiteId.value) await refreshStats();
}

async function handleSiteSelection(value: number | string) {
  await workspace.selectSite(value);
  await refreshStats();
}

async function handleRangeSelection(range: AnalyticsRange) {
  rangeDays.value = range.days;
  rangeDates.value = range.dates;
  await refreshStats();
}

function formatNumber(value: number) {
  return new Intl.NumberFormat().format(value);
}

function metric(value: number | undefined, suffix = "") {
  if (!hasLoadedSite.value || value === undefined) return "—";
  return `${formatNumber(value)}${suffix}`;
}
</script>

<template>
  <a class="skip-link" href="#main-content">Skip to content</a>

  <main v-if="sessionPending" id="main-content" class="auth-shell">
    <section class="auth-panel auth-loading" aria-live="polite">
      <BrandLockup />
      <span class="state-orbit" aria-hidden="true"></span>
      <p>Checking your session…</p>
    </section>
  </main>

  <ConsoleSignIn
    v-else-if="!isAuthenticated"
    :api-base="apiBase"
    :error="authError"
    :initial-challenge-id="authChallengeId"
    :privacy-notice-url="privacyNoticeUrl"
    :terms-url="termsUrl"
    @authenticated="handleAuthenticated"
    @clear-error="authQueryError = ''"
  />

  <FirstRunOnboarding
    v-else-if="isAuthenticated && user && !isDemoUser && onboardingStep"
    :api-base="apiBase"
    :email="user.email"
    :sign-out-error="signOutError"
    :sign-out-pending="signOutPending"
    :step="onboardingStep"
    @changed="handleOnboardingChanged"
    @finished="finishOnboarding"
    @sign-out="signOut"
  />

  <div
    v-else
    class="console-shell"
    :class="{ 'sidebar-collapsed': sidebarCollapsed }"
  >
    <ConsoleSidebar
      v-if="user"
      v-model:collapsed="sidebarCollapsed"
      :current-role="selectedRole"
      :demo="isDemoUser"
      :entitlements="selectedSite?.entitlements"
      :permissions="selectedPermissions"
      :site-id="selectedSite?.tracking_id"
      :sign-out-pending="signOutPending"
      :user="user"
      @sign-out="signOut"
    />

    <main id="main-content" class="workspace">
      <AppNavigation
        :model-value="siteId"
        :options="siteOptions"
        @change="handleSiteSelection"
      />
      <header id="overview" class="page-header">
        <div class="page-title">
          <p class="eyebrow">Analytics console</p>
          <h1>Overview</h1>
          <p>
            {{
              statsPending && stats
                ? `Updating to ${rangeDays} days…`
                : `${loadedRangeDays} days of privacy-preserving product analytics.`
            }}
          </p>
        </div>
      </header>

      <p v-if="signOutError" class="page-alert" role="alert">
        {{ signOutError }}
      </p>

      <div v-if="sitesPending" class="site-toolbar site-loading" role="status">
        <span class="state-orbit" aria-hidden="true"></span>
        <span>Loading your sites…</span>
      </div>

      <section
        v-else-if="!sites.length && !sitesError"
        class="site-empty"
        aria-labelledby="no-sites"
      >
        <div>
          <p class="eyebrow">
            {{ isDemoUser ? "Apps unavailable" : "No sites yet" }}
          </p>
          <h2 id="no-sites">
            {{
              isDemoUser
                ? "Your apps missed their cue."
                : "Create your first site to start tracking."
            }}
          </h2>
          <p>
            {{
              isDemoUser
                ? "Retry loading the app catalog from the API."
                : "Add its primary domain and timezone, then use the generated tracking ID in the SDK."
            }}
          </p>
          <button
            v-if="isDemoUser"
            class="button primary compact"
            type="button"
            @click="refreshSites()"
          >
            Retry app catalog
          </button>
          <NuxtLink v-else class="button primary compact" to="/app/create">
            Create an app
          </NuxtLink>
        </div>
      </section>

      <form
        v-else
        class="site-toolbar overview-filters"
        aria-label="Analytics date range and refresh"
        @submit.prevent="refreshStats(true)"
      >
        <AnalyticsRangeControl @change="handleRangeSelection" />
        <TrackingSetupReveal
          v-if="selectedSite"
          :api-base="apiBase"
          :site-id="selectedSite.tracking_id"
        />
        <p v-else id="site-help">
          Use the tracking ID from your OWLEYE site settings.
        </p>
        <button
          class="button primary compact overview-refresh"
          :class="{ 'is-refreshing': statsPending }"
          :disabled="!canLoadStats || statsPending"
          :aria-label="
            statsPending
              ? 'Loading analytics'
              : hasLoadedSite
                ? 'Refresh analytics'
                : 'Load analytics'
          "
          :title="
            statsPending
              ? 'Loading analytics'
              : hasLoadedSite
                ? 'Refresh analytics'
                : 'Load analytics'
          "
          :aria-busy="statsPending"
          type="submit"
        >
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M20 7v5h-5M4 17v-5h5" />
            <path
              d="M6.1 7a7 7 0 0 1 11.55-1L20 9M4 15l2.35 3A7 7 0 0 0 17.9 17"
            />
          </svg>
        </button>
      </form>

      <p v-if="sitesError" class="page-alert" role="alert">{{ sitesError }}</p>
      <p v-if="statsError" class="page-alert" role="alert">{{ statsError }}</p>

      <section
        class="status-strip"
        aria-label="Overview metrics"
        :aria-busy="statsPending"
      >
        <article>
          <span>Page views</span>
          <strong :class="{ skeleton: statsPending && !stats }">{{
            metric(stats?.totals.pageviews)
          }}</strong>
          <small>{{ rangeMetricCaption }}</small>
        </article>
        <article>
          <span>Unique visitors</span>
          <strong :class="{ skeleton: statsPending && !stats }">{{
            metric(stats?.totals.visitors)
          }}</strong>
          <small>
            {{
              statsPending && stats
                ? rangeMetricCaption
                : "Anonymous, deduplicated"
            }}
          </small>
        </article>
        <article>
          <span>Total events tracked</span>
          <strong :class="{ skeleton: statsPending && !stats }">{{
            metric(stats?.totals.events)
          }}</strong>
          <small>{{ rangeMetricCaption }}</small>
        </article>
      </section>

      <section class="content-grid">
        <article id="traffic" class="panel wide data-stage">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Trend</p>
              <h2>Traffic</h2>
            </div>
            <span>{{
              stats
                ? `${stats.start_date} – ${stats.end_date} · ${{ day: "Daily", week: "Weekly", month: "Monthly" }[stats.interval ?? analyticsInterval(stats.days)]} (UTC)`
                : `${rangeDays}-day range`
            }}</span>
          </div>
          <OverviewChart
            v-if="normalizedSiteId"
            :error="statsError"
            :loading="statsPending"
            :points="stats?.timeseries ?? []"
            :interval="stats?.interval ?? 'day'"
            :range-key="
              stats
                ? `${stats.site_id}:${stats.start_date}:${stats.end_date}:${stats.interval}`
                : normalizedSiteId
            "
            @retry="refreshStats"
          />
          <div v-else class="chart-state" role="status">
            <span class="state-symbol" aria-hidden="true">↗</span>
            <strong>Select a site to view traffic</strong>
            <span>
              {{
                isDemoUser
                  ? "Choose one of the apps above."
                  : "Enter its tracking ID above. OwlEye never substitutes sample analytics."
              }}
            </span>
          </div>
        </article>

        <article class="panel">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Totals</p>
              <h2>Summary</h2>
            </div>
            <span>{{ hasLoadedSite ? stats?.site_id : "No site loaded" }}</span>
          </div>
          <div class="stat-list">
            <div>
              <span>Sessions</span>
              <strong :class="{ skeleton: statsPending && !stats }">{{
                metric(stats?.totals.sessions)
              }}</strong>
            </div>
            <div>
              <span>External events</span>
              <strong :class="{ skeleton: statsPending && !stats }">{{
                metric(stats?.totals.external_events)
              }}</strong>
            </div>
            <div>
              <span>Rule events</span>
              <strong :class="{ skeleton: statsPending && !stats }">{{
                metric(stats?.totals.rule_events)
              }}</strong>
            </div>
            <div>
              <span
                title="Average across events with a duration, including custom timings and Web Vitals"
                >Avg. event duration</span
              >
              <strong :class="{ skeleton: statsPending && !stats }">{{
                metric(stats?.totals.avg_duration_ms, " ms")
              }}</strong>
            </div>
          </div>
        </article>

        <article class="panel">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Content</p>
              <h2>Top pages</h2>
            </div>
          </div>
          <div v-if="statsPending" class="list-skeleton" role="status">
            <span>Loading top pages…</span>
          </div>
          <div v-else-if="stats?.top_pages.length" class="rank-list">
            <div v-for="page in stats.top_pages" :key="page.path">
              <span :title="page.title || page.path">{{ page.path }}</span>
              <strong>{{ formatNumber(page.views) }}</strong>
            </div>
          </div>
          <p v-else class="empty-state">
            {{
              stats
                ? "No page views in this range."
                : normalizedSiteId
                  ? "Load this site to see its top pages."
                  : "Select a site to see its top pages."
            }}
          </p>
        </article>

        <article class="panel wide geo-panel">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Geography</p>
              <h2>Countries</h2>
            </div>
            <BreakdownMetricToggle
              v-model="breakdownMetric"
              label="Geography metric"
            />
          </div>
          <LazyCountryGeoChart
            v-if="normalizedSiteId"
            :countries="breakdownItems(stats?.countries, breakdownMetric)"
            :metric-label="metricLabel"
            :error="statsError"
            hydrate-on-visible
            :loading="statsPending"
            @retry="refreshStats"
          />
          <div v-else class="chart-state" role="status">
            <span class="state-symbol" aria-hidden="true">◎</span>
            <strong>Select a site to map its audience</strong>
            <span
              >Country totals stay empty until the API returns real
              analytics.</span
            >
          </div>
        </article>

        <article id="audience" class="panel wide dimension-panel">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Audience &amp; technology</p>
              <h2>How people showed up</h2>
            </div>
            <BreakdownMetricToggle
              v-model="breakdownMetric"
              label="Audience metric"
            />
          </div>
          <p class="breakdown-note">
            {{
              breakdownMetric === "visitors"
                ? "Visitors with a page view, deduplicated across this range. A visitor can appear in multiple categories; the center total counts them once."
                : "Page views only. Custom, rule, session, and performance events are excluded."
            }}
            The metric selection also applies to geography and acquisition.
          </p>
          <div class="dimension-chart-grid">
            <section aria-labelledby="browser-chart-title">
              <h3 id="browser-chart-title">Browsers</h3>
              <LazyDimensionDonutChart
                :error="statsError"
                hydrate-on-visible
                :items="breakdownItems(stats?.browsers, breakdownMetric)"
                :metric-label="metricLabel"
                :total="audienceTotal"
                label="Browser traffic"
                :loading="statsPending"
                @retry="refreshStats"
              />
            </section>
            <section aria-labelledby="os-chart-title">
              <h3 id="os-chart-title">Operating systems</h3>
              <LazyDimensionDonutChart
                empty-message="No operating-system data in this range."
                :error="statsError"
                hydrate-on-visible
                :items="
                  breakdownItems(stats?.operating_systems, breakdownMetric)
                "
                :metric-label="metricLabel"
                :total="audienceTotal"
                label="Operating-system traffic"
                :loading="statsPending"
                @retry="refreshStats"
              />
            </section>
            <section aria-labelledby="device-chart-title">
              <h3 id="device-chart-title">Devices</h3>
              <LazyDimensionDonutChart
                empty-message="No device data in this range."
                :error="statsError"
                hydrate-on-visible
                :items="breakdownItems(stats?.devices, breakdownMetric)"
                :metric-label="metricLabel"
                :total="audienceTotal"
                label="Device traffic"
                :loading="statsPending"
                @retry="refreshStats"
              />
            </section>
          </div>
        </article>

        <article class="panel">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Acquisition</p>
              <h2>Referrers</h2>
            </div>
            <span>{{ metricLabel }}</span>
          </div>
          <div v-if="statsPending" class="list-skeleton" role="status">
            <span>Loading referrers…</span>
          </div>
          <div v-else-if="stats?.referrers?.length" class="rank-list">
            <div
              v-for="referrer in breakdownItems(
                stats.referrers,
                breakdownMetric,
              )"
              :key="referrer.name"
            >
              <span>{{ referrer.name }}</span>
              <strong>{{ formatNumber(referrer.count) }}</strong>
            </div>
          </div>
          <p v-else class="empty-state">
            {{
              stats
                ? "No referrer data in this range."
                : "Load a site to see referrers."
            }}
          </p>
        </article>

        <article class="panel">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Acquisition</p>
              <h2>UTM campaigns</h2>
            </div>
            <span>{{ metricLabel }}</span>
          </div>
          <div v-if="statsPending" class="list-skeleton" role="status">
            <span>Loading campaigns…</span>
          </div>
          <div v-else-if="stats?.utm_campaigns?.length" class="rank-list">
            <div
              v-for="campaign in breakdownItems(
                stats.utm_campaigns,
                breakdownMetric,
              )"
              :key="campaign.name"
            >
              <span>{{ campaign.name }}</span>
              <strong>{{ formatNumber(campaign.count) }}</strong>
            </div>
          </div>
          <p v-else class="empty-state">
            {{
              stats
                ? "No UTM campaigns in this range."
                : "Load a site to see campaign traffic."
            }}
          </p>
        </article>

        <article id="events" class="panel">
          <div class="panel-header">
            <div>
              <p class="panel-kicker">Signals</p>
              <h2>Events</h2>
            </div>
          </div>
          <div v-if="statsPending" class="list-skeleton" role="status">
            <span>Loading events…</span>
          </div>
          <div v-else-if="stats?.event_names.length" class="rank-list">
            <div
              v-for="event in stats.event_names"
              :key="`${event.event_type}:${event.event_name}`"
            >
              <span
                >{{ event.event_type }} /
                {{ event.event_name || "unnamed" }}</span
              >
              <strong>{{ formatNumber(event.count) }}</strong>
            </div>
          </div>
          <p v-else class="empty-state">
            {{
              stats
                ? "No events in this range."
                : normalizedSiteId
                  ? "Load this site to see tracked events."
                  : "Select a site to see tracked events."
            }}
          </p>
        </article>
      </section>
    </main>
  </div>
</template>

<style scoped>
.breakdown-note {
  color: var(--text-secondary);
  font-size: 0.85rem;
  line-height: 1.6;
  margin: 0 0 20px;
}
.site-toolbar.overview-filters {
  grid-template-columns: minmax(200px, 1fr) auto auto;
  align-items: center;
  gap: 7px 14px;
}
.overview-filters :deep(.analytics-range-control) {
  display: grid;
  grid-column: 1;
  grid-row: 1 / span 3;
  grid-template-rows: subgrid;
}
.overview-filters :deep(.site-control) {
  display: grid;
  grid-row: 1 / span 2;
  grid-template-rows: subgrid;
}
.overview-filters :deep(.tracking-identity),
.overview-filters > #site-help {
  grid-column: 2;
  grid-row: 2;
  margin: 0;
}
.overview-filters > .overview-refresh {
  grid-column: 3;
  grid-row: 2;
  width: 44px;
  height: 44px;
  min-height: 44px;
  padding: 0;
  justify-self: end;
}
.overview-filters :deep(.tracking-recipe) {
  grid-column: 1 / -1;
  grid-row: 4;
}
@media (max-width: 1100px) {
  .site-toolbar.overview-filters {
    grid-template-columns: minmax(0, 1fr) auto;
  }
  .overview-filters > .overview-refresh {
    grid-column: 2;
  }
  .overview-filters :deep(.tracking-identity),
  .overview-filters > #site-help {
    grid-column: 1 / -1;
    grid-row: 4;
  }
  .overview-filters :deep(.tracking-recipe) {
    grid-row: 5;
  }
}
@media (max-width: 600px) {
  .site-toolbar.overview-filters {
    grid-template-columns: minmax(0, 1fr) auto;
    row-gap: 14px;
  }
  .overview-filters :deep(.analytics-range-control),
  .overview-filters :deep(.site-control) {
    display: block;
    grid-row: auto;
  }
  .overview-filters :deep(.analytics-range-control) {
    grid-column: 1 / -1;
  }
  .overview-filters :deep(.site-control > span) {
    display: block;
    margin-bottom: 7px;
  }
  .overview-filters :deep(.tracking-identity),
  .overview-filters > #site-help {
    grid-column: 1;
    grid-row: 2;
  }
  .overview-filters > .overview-refresh {
    grid-column: 2;
    grid-row: 2;
  }
  .overview-filters :deep(.tracking-recipe) {
    grid-column: 1 / -1;
    grid-row: 3;
  }
}
.overview-refresh.is-refreshing svg {
  animation: refresh-spin 1s linear infinite;
}
@keyframes refresh-spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .overview-refresh.is-refreshing svg {
    animation: none;
  }
}
</style>
