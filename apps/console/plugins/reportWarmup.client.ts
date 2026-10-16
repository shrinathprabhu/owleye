import {
  PREVIEW_USER_ID,
  sitePermissions,
  type WorkspaceSession,
  type WorkspaceSite,
} from "../types/workspace.ts";
import { reportWarmups } from "../utils/reportWarmup.ts";

/** Warm a few default reports once; never enumerate all API responses. */
export default defineNuxtPlugin((app) => {
  const route = useRoute();
  const session = useState<WorkspaceSession | null>(
    "console-workspace-session",
    () => null,
  );
  const sites = useState<WorkspaceSite[]>("console-workspace-sites", () => []);
  const selected = useState<string>(
    "console-workspace-selected-site",
    () => "",
  );
  const validated = useState<number>("console-workspace-validated-at", () => 0);
  const { warmReport, reportWarmupQueue } = useApi();
  let mounted = false;
  function schedule() {
    // Public shares and account/setup pages never trigger authenticated report
    // warmup, even after navigating there from a signed-in Console route.
    if (
      ![
        "/",
        "/events",
        "/campaigns",
        "/performance",
        "/rules",
        "/funnels",
        "/pro-view",
        "/live",
      ].includes(route.path)
    ) {
      reportWarmupQueue.clear();
      return;
    }
    const current = session.value;
    if (
      !mounted ||
      !validated.value ||
      !current?.authenticated ||
      current.user.id === PREVIEW_USER_ID ||
      current.onboarding.next_step !== "complete" ||
      navigator.onLine === false ||
      document.visibilityState === "hidden"
    )
      return;
    const connection = (
      navigator as Navigator & {
        connection?: { saveData?: boolean; effectiveType?: string };
      }
    ).connection;
    if (
      connection?.saveData ||
      /(^|-)2g$/.test(connection?.effectiveType ?? "")
    )
      return;
    const site = sites.value.find(
      (item) =>
        item.id === selected.value || item.tracking_id === selected.value,
    );
    if (!site || !sitePermissions(site).analytics_read) return;
    for (const report of reportWarmups(site.tracking_id, route.path))
      warmReport(report);
  }
  const stop = watch(
    [session, sites, selected, validated, () => route.path],
    schedule,
    { deep: true, flush: "post" },
  );
  const visibility = () => {
    if (document.visibilityState === "hidden") reportWarmupQueue.clear();
    else schedule();
  };
  app.hook("app:mounted", () => {
    mounted = true;
    document.addEventListener("visibilitychange", visibility);
    schedule();
  });
  app.vueApp.onUnmount(() => {
    stop();
    mounted = false;
    document.removeEventListener("visibilitychange", visibility);
    reportWarmupQueue.clear();
  });
});
