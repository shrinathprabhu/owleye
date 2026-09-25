import {
  computed,
  onBeforeUnmount,
  ref,
  shallowRef,
  watch,
  type Ref,
} from "vue";

import { widgetRequestBody } from "~/types/pro-view";

import type { StatsOverviewResponse } from "~/types/stats";
import type {
  ProViewDashboard,
  ProViewDeleteScope,
  ProViewEventOption,
  ProViewPreviewResponse,
  ProViewShare,
  ProViewShareCandidate,
  ProViewWidgetDefinition,
} from "~/types/pro-view";
import type { WorkspaceSite } from "~/types/workspace";
import {
  apiErrorMessage as requestError,
  isAbortError,
} from "~/utils/apiError";
import { routes } from "~/utils/routes";

type RuleSummary = {
  enabled: boolean;
  id: string;
  name: string;
  type: string;
};

type ProViewWorkspaceOptions = {
  canWrite: Readonly<Ref<boolean>>;
  demo: Readonly<Ref<boolean>>;
  site: Readonly<Ref<WorkspaceSite | null>>;
};

/**
 * Owns Pro View's API-backed definitions and previews. Authentication, app
 * selection, and navigation intentionally stay in the shared console shell.
 */
export function useProViewWorkspace(options: ProViewWorkspaceOptions) {
  const { api } = useApi();
  const dashboards = shallowRef<ProViewDashboard[]>([]);
  const pendingInvites = shallowRef<ProViewDashboard[]>([]);
  const activeDashboard = shallowRef<ProViewDashboard | null>(null);
  const invitePreview = shallowRef<ProViewDashboard | null>(null);
  const shareCandidates = shallowRef<ProViewShareCandidate[]>([]);
  const eventOptions = shallowRef<ProViewEventOption[]>([]);
  const preview = shallowRef<ProViewPreviewResponse | null>(null);
  const workspacePending = ref(false);
  const mutationPending = ref(false);
  const previewPending = ref(false);
  const invitePreviewPending = ref(false);
  const errorMessage = ref("");
  const notice = ref("");
  const previewError = ref("");
  const persistenceStatus = ref<
    "available" | "checking" | "error" | "unavailable"
  >("checking");

  let siteRequest = 0;
  let siteContext = 0;
  let mutationOperation = 0;
  let workspaceController: AbortController | undefined;
  let dashboardController: AbortController | undefined;
  let inviteController: AbortController | undefined;
  let previewController: AbortController | undefined;

  const canPersist = computed(
    () =>
      persistenceStatus.value === "available" &&
      options.canWrite.value &&
      !options.demo.value &&
      (activeDashboard.value?.capabilities?.can_edit ?? true),
  );

  watch(
    options.site,
    () => {
      siteContext += 1;
      mutationOperation += 1;
      previewController?.abort();
      dashboardController?.abort();
      inviteController?.abort();
      mutationPending.value = false;
      preview.value = null;
      previewError.value = "";
      void loadSelectedSite();
    },
    { immediate: true },
  );

  onBeforeUnmount(() => {
    workspaceController?.abort();
    dashboardController?.abort();
    inviteController?.abort();
    previewController?.abort();
  });

  async function loadSelectedSite() {
    const site = options.site.value;
    const request = ++siteRequest;
    const preferredDashboardId = activeDashboard.value?.id;
    workspaceController?.abort();
    dashboardController?.abort();
    inviteController?.abort();
    workspaceController = undefined;
    dashboards.value = [];
    pendingInvites.value = [];
    activeDashboard.value = null;
    invitePreview.value = null;
    shareCandidates.value = [];
    eventOptions.value = [];
    errorMessage.value = "";
    notice.value = "";

    if (!site) {
      persistenceStatus.value = "checking";
      workspacePending.value = false;
      return;
    }

    const controller = new AbortController();
    workspaceController = controller;
    workspacePending.value = true;
    persistenceStatus.value = "checking";

    try {
      const [catalogResult, dashboardResult, inviteResult] =
        await Promise.allSettled([
          loadEventCatalog(site, controller.signal),
          loadDashboards(site, controller.signal),
          loadPendingInvites(site, controller.signal),
        ]);
      if (request !== siteRequest) return;

      if (catalogResult.status === "fulfilled") {
        eventOptions.value = catalogResult.value;
      } else if (!isAbortError(catalogResult.reason)) {
        errorMessage.value = requestError(
          catalogResult.reason,
          "Tracked events and rules could not be loaded.",
        );
      }

      if (dashboardResult.status === "fulfilled") {
        persistenceStatus.value = "available";
        dashboards.value = dashboardResult.value;
        const dashboard =
          dashboardResult.value.find(
            (item) => item.id === preferredDashboardId,
          ) ??
          dashboardResult.value.find((item) => item.is_default) ??
          dashboardResult.value[0] ??
          null;
        if (dashboard) {
          const detail = dashboard.widgets
            ? dashboard
            : await loadDashboardDetail(site, dashboard, controller.signal);
          if (request !== siteRequest) return;
          activeDashboard.value = detail;
          if (detail.capabilities?.can_share) {
            try {
              shareCandidates.value = await loadShareCandidates(
                site,
                controller.signal,
              );
            } catch (error) {
              if (!isAbortError(error) && request === siteRequest) {
                errorMessage.value ||= requestError(
                  error,
                  "App members could not be loaded for sharing.",
                );
              }
            }
          }
        }
      } else if (isNotFound(dashboardResult.reason)) {
        persistenceStatus.value = "unavailable";
      } else if (!isAbortError(dashboardResult.reason)) {
        persistenceStatus.value = "error";
        errorMessage.value ||= requestError(
          dashboardResult.reason,
          "Saved Pro Views could not be loaded.",
        );
      }

      if (inviteResult.status === "fulfilled") {
        pendingInvites.value = inviteResult.value;
      } else if (!isAbortError(inviteResult.reason)) {
        errorMessage.value ||= requestError(
          inviteResult.reason,
          "Pending Pro View invites could not be loaded.",
        );
      }
    } catch (error) {
      if (!isAbortError(error) && request === siteRequest) {
        persistenceStatus.value = "error";
        errorMessage.value = requestError(
          error,
          "The selected Pro View could not be loaded.",
        );
      }
    } finally {
      if (request === siteRequest) workspacePending.value = false;
      if (workspaceController === controller) workspaceController = undefined;
    }
  }

  async function selectDashboard(dashboardId: string) {
    const site = options.site.value;
    const dashboard = dashboards.value.find((item) => item.id === dashboardId);
    if (!site || !dashboard || activeDashboard.value?.id === dashboardId)
      return;

    const context = siteContext;
    dashboardController?.abort();
    const controller = new AbortController();
    dashboardController = controller;
    workspacePending.value = true;
    errorMessage.value = "";
    notice.value = "";

    try {
      const detail = dashboard.widgets
        ? dashboard
        : await loadDashboardDetail(site, dashboard, controller.signal);
      if (
        controller.signal.aborted ||
        dashboardController !== controller ||
        !isCurrentSiteContext(site, context)
      ) {
        return;
      }
      activeDashboard.value = detail;
      const candidates = detail.capabilities?.can_share
        ? await loadShareCandidates(site, controller.signal)
        : [];
      if (
        controller.signal.aborted ||
        dashboardController !== controller ||
        !isCurrentSiteContext(site, context)
      ) {
        return;
      }
      shareCandidates.value = candidates;
    } catch (error) {
      if (
        !isAbortError(error) &&
        dashboardController === controller &&
        isCurrentSiteContext(site, context)
      ) {
        errorMessage.value = requestError(
          error,
          "That Pro View could not be loaded.",
        );
      }
    } finally {
      if (dashboardController === controller) {
        dashboardController = undefined;
        workspacePending.value = false;
      }
    }
  }

  async function previewInvite(dashboardId: string) {
    const site = options.site.value;
    const invite = pendingInvites.value.find((item) => item.id === dashboardId);
    if (!site || !invite) return;
    const context = siteContext;
    inviteController?.abort();
    const controller = new AbortController();
    inviteController = controller;
    invitePreviewPending.value = true;
    errorMessage.value = "";

    try {
      const response = await api<ProViewDashboard>(
        routes.sites.dashboardInvite(site.tracking_id, dashboardId),
        {
          signal: controller.signal,
        },
      );
      if (
        controller.signal.aborted ||
        inviteController !== controller ||
        !isCurrentSiteContext(site, context)
      ) {
        return;
      }
      invitePreview.value = response;
    } catch (error) {
      if (
        !isAbortError(error) &&
        inviteController === controller &&
        isCurrentSiteContext(site, context)
      ) {
        errorMessage.value = requestError(
          error,
          "That shared Pro View preview could not be loaded.",
        );
      }
    } finally {
      if (inviteController === controller) {
        inviteController = undefined;
        invitePreviewPending.value = false;
      }
    }
  }

  async function previewDefinition(definition: ProViewWidgetDefinition) {
    const site = options.site.value;
    if (!site) return;
    const context = siteContext;
    const dashboard = activeDashboard.value;
    previewController?.abort();
    const controller = new AbortController();
    previewController = controller;
    preview.value = null;
    previewError.value = "";
    previewPending.value = true;

    try {
      const path = dashboard
        ? routes.sites.dashboardWidgetPreview(site.tracking_id, dashboard.id)
        : routes.sites.dashboardPreview(site.tracking_id);
      const response = await api<ProViewPreviewResponse>(path, {
        body: widgetRequestBody(definition),
        method: "POST",
        signal: controller.signal,
      });
      if (
        controller.signal.aborted ||
        previewController !== controller ||
        !isCurrentSiteContext(site, context)
      )
        return;
      preview.value = response;
    } catch (error) {
      if (
        !isAbortError(error) &&
        previewController === controller &&
        isCurrentSiteContext(site, context)
      ) {
        previewError.value = isNotFound(error)
          ? "This preview is unavailable. Reload the page to refresh the app and dashboard, then try again."
          : requestError(
              error,
              "Couldn’t reach the analytics service. Check your connection and retry.",
            );
      }
    } finally {
      if (previewController === controller) {
        previewController = undefined;
        previewPending.value = false;
      }
    }
  }

  async function saveDefinition(definition: ProViewWidgetDefinition) {
    const site = options.site.value;
    if (!site || !canPersist.value || mutationPending.value) return false;
    const context = siteContext;
    const operation = ++mutationOperation;
    mutationPending.value = true;
    errorMessage.value = "";
    notice.value = "";

    try {
      let dashboard = activeDashboard.value;
      if (!dashboard) {
        dashboard = await api<ProViewDashboard>(
          routes.sites.dashboards(site.tracking_id),
          {
            body: { is_default: true, name: "My Pro View" },
            method: "POST",
          },
        );
        if (!isCurrentSiteContext(site, context)) return false;
        activeDashboard.value = dashboard;
      }

      const path = definition.id
        ? routes.sites.dashboardWidget(
            site.tracking_id,
            dashboard.id,
            definition.id,
          )
        : routes.sites.dashboardWidgets(site.tracking_id, dashboard.id);
      await api(path, {
        body: widgetRequestBody(definition),
        method: definition.id ? "PUT" : "POST",
      });
      if (!isCurrentSiteContext(site, context)) return false;
      await loadSelectedSite();
      if (!isCurrentSiteContext(site, context)) return false;
      notice.value = definition.id ? "Widget updated." : "Widget added.";
      return true;
    } catch (error) {
      if (!isCurrentSiteContext(site, context)) return false;
      errorMessage.value = requestError(
        error,
        "This widget could not be saved.",
      );
      return false;
    } finally {
      if (operation === mutationOperation) mutationPending.value = false;
    }
  }

  async function deleteDefinition(widget: ProViewWidgetDefinition) {
    const site = options.site.value;
    const dashboard = activeDashboard.value;
    if (
      !site ||
      !dashboard ||
      !widget.id ||
      !canPersist.value ||
      mutationPending.value
    ) {
      return false;
    }
    const context = siteContext;
    const operation = ++mutationOperation;
    mutationPending.value = true;
    errorMessage.value = "";
    notice.value = "";
    try {
      await api(
        routes.sites.dashboardWidget(site.tracking_id, dashboard.id, widget.id),
        {
          method: "DELETE",
        },
      );
      if (!isCurrentSiteContext(site, context)) return false;
      await loadSelectedSite();
      if (!isCurrentSiteContext(site, context)) return false;
      notice.value = "Widget removed.";
      return true;
    } catch (error) {
      if (!isCurrentSiteContext(site, context)) return false;
      errorMessage.value = requestError(
        error,
        "This widget could not be removed.",
      );
      return false;
    } finally {
      if (operation === mutationOperation) mutationPending.value = false;
    }
  }

  async function inviteViewer(userId: string) {
    const dashboard = activeDashboard.value;
    if (!dashboard?.capabilities?.can_share || !userId) return false;

    return performMutation(
      "That viewer could not be invited.",
      (site) =>
        api<ProViewShare[]>(
          routes.sites.dashboardShares(site.tracking_id, dashboard.id),
          {
            body: { user_ids: [userId] },
            method: "POST",
          },
        ),
      (shares) => {
        if (activeDashboard.value?.id !== dashboard.id) return;
        activeDashboard.value = { ...activeDashboard.value, shares };
      },
      "Viewer invited to preview.",
    );
  }

  async function revokeShare(userId: string) {
    const dashboard = activeDashboard.value;
    if (!dashboard?.capabilities?.can_revoke_for_others || !userId) {
      return false;
    }

    return performMutation(
      "That shared copy could not be revoked.",
      (site) =>
        api(
          routes.sites.dashboardShare(site.tracking_id, dashboard.id, userId),
          {
            method: "DELETE",
          },
        ),
      () => {
        if (activeDashboard.value?.id !== dashboard.id) return;
        activeDashboard.value = {
          ...activeDashboard.value,
          shares: activeDashboard.value.shares?.map((share) =>
            share.user_id === userId ? { ...share, status: "revoked" } : share,
          ),
        };
      },
      "Shared copy revoked.",
    );
  }

  async function respondToInvite(
    dashboardId: string,
    decision: "accept" | "dismiss",
  ) {
    const invite = pendingInvites.value.find((item) => item.id === dashboardId);
    if (!invite) return false;

    return performMutation(
      decision === "accept"
        ? "That Pro View could not be accepted."
        : "That Pro View could not be dismissed.",
      (site) =>
        api<ProViewDashboard | { dashboard_id: string; status: string }>(
          routes.sites.dashboardInviteDecision(
            site.tracking_id,
            dashboardId,
            decision,
          ),
          {
            method: "POST",
          },
        ),
      (response) => {
        pendingInvites.value = pendingInvites.value.filter(
          (item) => item.id !== dashboardId,
        );
        invitePreview.value = null;
        if (decision === "accept" && "id" in response) {
          dashboards.value = [
            ...dashboards.value.filter((item) => item.id !== response.id),
            response,
          ];
          activeDashboard.value = response;
          shareCandidates.value = [];
        }
      },
      decision === "accept"
        ? "Shared Pro View kept in your workspace."
        : "Invite dismissed.",
    );
  }

  async function deleteDashboard(scope: ProViewDeleteScope = "all") {
    if (scope === "self") return removeDashboardForMe();
    if (scope === "others") return revokeAllShares();
    return deleteDashboardForAll();
  }

  async function removeDashboardForMe() {
    const dashboard = activeDashboard.value;
    if (!dashboard?.capabilities?.can_remove_for_me) return false;

    return performMutation(
      "This shared Pro View could not be removed from your workspace.",
      (site) =>
        api(routes.sites.dashboardShareSelf(site.tracking_id, dashboard.id), {
          method: "DELETE",
        }),
      async () => {
        dashboards.value = dashboards.value.filter(
          (item) => item.id !== dashboard.id,
        );
        activeDashboard.value = null;
        const nextDashboard =
          dashboards.value.find((item) => item.is_default) ??
          dashboards.value[0] ??
          null;
        if (nextDashboard) await selectDashboard(nextDashboard.id);
      },
      "Shared Pro View removed from your workspace.",
    );
  }

  async function revokeAllShares() {
    const dashboard = activeDashboard.value;
    if (!dashboard?.capabilities?.can_revoke_for_others) return false;

    return performMutation(
      "Shared copies could not be revoked.",
      (site) =>
        api(routes.sites.dashboardShares(site.tracking_id, dashboard.id), {
          method: "DELETE",
        }),
      () => {
        if (activeDashboard.value?.id !== dashboard.id) return;
        activeDashboard.value = {
          ...activeDashboard.value,
          shares: activeDashboard.value.shares?.map((share) => ({
            ...share,
            status: "revoked",
          })),
        };
      },
      "All shared copies revoked. Your Pro View is still here.",
    );
  }

  async function deleteDashboardForAll() {
    const site = options.site.value;
    const dashboard = activeDashboard.value;
    if (
      !site ||
      !dashboard ||
      options.demo.value ||
      mutationPending.value ||
      !(dashboard.capabilities?.can_delete ?? false)
    ) {
      return false;
    }
    const context = siteContext;
    const operation = ++mutationOperation;
    mutationPending.value = true;
    errorMessage.value = "";
    notice.value = "";
    try {
      await api(routes.sites.dashboard(site.tracking_id, dashboard.id), {
        method: "DELETE",
      });
      if (!isCurrentSiteContext(site, context)) return false;
      await loadSelectedSite();
      if (!isCurrentSiteContext(site, context)) return false;
      notice.value = "Pro View deleted.";
      return true;
    } catch (error) {
      if (!isCurrentSiteContext(site, context)) return false;
      errorMessage.value = requestError(
        error,
        "This Pro View could not be deleted.",
      );
      return false;
    } finally {
      if (operation === mutationOperation) mutationPending.value = false;
    }
  }

  async function performMutation<T>(
    fallback: string,
    request: (site: WorkspaceSite) => Promise<T>,
    onSuccess: (response: T) => Promise<void> | void,
    success: string,
  ) {
    const site = options.site.value;
    if (!site || options.demo.value || mutationPending.value) return false;
    const context = siteContext;
    const operation = ++mutationOperation;
    mutationPending.value = true;
    errorMessage.value = "";
    notice.value = "";

    try {
      const response = await request(site);
      if (!isCurrentSiteContext(site, context)) return false;
      await onSuccess(response);
      if (!isCurrentSiteContext(site, context)) return false;
      notice.value = success;
      return true;
    } catch (error) {
      if (!isCurrentSiteContext(site, context)) return false;
      errorMessage.value = requestError(error, fallback);
      return false;
    } finally {
      if (operation === mutationOperation) mutationPending.value = false;
    }
  }

  async function loadEventCatalog(site: WorkspaceSite, signal: AbortSignal) {
    const [stats, rules] = await Promise.all([
      api<StatsOverviewResponse>("/v1/stats/overview", {
        query: { days: 30, site_id: site.tracking_id },
        signal,
      }),
      api<RuleSummary[]>(routes.sites.rules(site.tracking_id), {
        signal,
      }).catch((error) => {
        if (isAbortError(error)) throw error;
        return [];
      }),
    ]);

    return dedupeCatalog([
      ...stats.event_names.map<ProViewEventOption>((event) => ({
        hasLocation: false,
        id: event.event_name,
        kind: "event",
        label: event.event_name,
        meta: `${event.event_type} · ${new Intl.NumberFormat().format(event.count)} in 30 days`,
      })),
      ...rules.map<ProViewEventOption>((rule) => ({
        hasLocation: false,
        id: rule.id,
        kind: "rule",
        label: rule.name,
        meta: `${rule.type} rule · ${rule.enabled ? "enabled" : "paused"}`,
      })),
    ]);
  }

  async function loadDashboards(site: WorkspaceSite, signal: AbortSignal) {
    return api<ProViewDashboard[]>(routes.sites.dashboards(site.tracking_id), {
      signal,
    });
  }

  async function loadPendingInvites(site: WorkspaceSite, signal: AbortSignal) {
    return api<ProViewDashboard[]>(
      routes.sites.dashboardInvites(site.tracking_id),
      {
        signal,
      },
    );
  }

  async function loadShareCandidates(site: WorkspaceSite, signal: AbortSignal) {
    const members = await api<ProViewShareCandidate[]>(
      routes.sites.members(site.tracking_id),
      {
        signal,
      },
    );
    return members.filter((member) => !member.is_current_user);
  }

  function loadDashboardDetail(
    site: WorkspaceSite,
    dashboard: ProViewDashboard,
    signal: AbortSignal,
  ) {
    return api<ProViewDashboard>(
      routes.sites.dashboard(site.tracking_id, dashboard.id),
      {
        signal,
      },
    );
  }

  function isCurrentSiteContext(site: WorkspaceSite, context: number) {
    return context === siteContext && options.site.value?.id === site.id;
  }

  return {
    activeDashboard,
    canPersist,
    dashboards,
    deleteDashboard,
    deleteDefinition,
    errorMessage,
    eventOptions,
    invitePreview,
    invitePreviewPending,
    inviteViewer,
    loadSelectedSite,
    mutationPending,
    notice,
    pendingInvites,
    persistenceStatus,
    preview,
    previewDefinition,
    previewError,
    previewPending,
    previewInvite,
    respondToInvite,
    revokeShare,
    saveDefinition,
    selectDashboard,
    shareCandidates,
    workspacePending,
  };
}

function dedupeCatalog(options: ProViewEventOption[]) {
  const seen = new Set<string>();
  return options.filter((option) => {
    const key = `${option.kind}:${option.id}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function isNotFound(error: unknown) {
  if (typeof error !== "object" || !error) return false;
  if ("status" in error && error.status === 404) return true;
  if ("statusCode" in error && error.statusCode === 404) return true;
  return (
    "response" in error &&
    (error as { response?: { status?: number } }).response?.status === 404
  );
}
