import { guarded, safely, cleanup, reportFailure } from "./safety";
const NAVIGATION_EVENT = "owleye:navigation";

const historyPatches = new WeakMap<History, HistoryPatch>();

export interface NavigationChange {
  from: string;
  to: string;
}

export function onNavigation(
  callback: (change: NavigationChange) => void,
): () => void {
  if (typeof window === "undefined") return () => undefined;

  const releaseHistoryPatch = acquireHistoryPatch();
  let currentUrl = window.location.href;

  const handleNavigation = guarded("SPA navigation", () => {
    const nextUrl = window.location.href;
    if (nextUrl === currentUrl) return;
    const previousUrl = currentUrl;
    currentUrl = nextUrl;
    callback({ from: previousUrl, to: nextUrl });
  });
  const remove = () =>
    cleanup(
      () => window.removeEventListener(NAVIGATION_EVENT, handleNavigation),
      () => window.removeEventListener("hashchange", handleNavigation),
      () => window.removeEventListener("popstate", handleNavigation),
      releaseHistoryPatch,
    );
  try {
    window.addEventListener(NAVIGATION_EVENT, handleNavigation);
    window.addEventListener("hashchange", handleNavigation);
    window.addEventListener("popstate", handleNavigation);
  } catch (error) {
    remove();
    throw error; // The caller's initialization guard logs and disables tracking.
  }
  return remove;
}

interface HistoryPatch {
  originalPushState: History["pushState"];
  originalReplaceState: History["replaceState"];
  patchedPushState: History["pushState"];
  patchedReplaceState: History["replaceState"];
  references: number;
}

function acquireHistoryPatch(): () => void {
  const history = window.history;
  const existing = historyPatches.get(history);
  if (existing) {
    existing.references += 1;
    return createHistoryRelease(history, existing);
  }

  const originalPushState = history.pushState;
  const originalReplaceState = history.replaceState;
  const patch: HistoryPatch = {
    originalPushState,
    originalReplaceState,
    patchedPushState: createPatchedHistoryMethod(originalPushState),
    patchedReplaceState: createPatchedHistoryMethod(originalReplaceState),
    references: 1,
  };

  try {
    history.pushState = patch.patchedPushState;
    history.replaceState = patch.patchedReplaceState;
  } catch {
    // Some hosts lock History methods. Native navigation events still work.
    reportFailure("SPA history instrumentation");
    safely("history restoration", () => {
      if (history.pushState === patch.patchedPushState)
        history.pushState = originalPushState;
    });
    return () => undefined;
  }
  historyPatches.set(history, patch);
  return createHistoryRelease(history, patch);
}

function createHistoryRelease(
  history: History,
  patch: HistoryPatch,
): () => void {
  let released = false;

  return () => {
    if (released) return;
    released = true;
    patch.references -= 1;
    if (patch.references > 0) return;

    cleanup(
      () => {
        if (history.pushState === patch.patchedPushState)
          history.pushState = patch.originalPushState;
      },
      () => {
        if (history.replaceState === patch.patchedReplaceState)
          history.replaceState = patch.originalReplaceState;
      },
    );
    historyPatches.delete(history);
  };
}

function createPatchedHistoryMethod(
  original: History["pushState"],
): History["pushState"] {
  return function patchedHistoryMethod(
    this: History,
    ...args: Parameters<History["pushState"]>
  ) {
    const previousUrl = safely(
      "navigation URL",
      () => window.location.href,
      "",
    );
    // Preserve native History behavior, including native exceptions. Only our
    // instrumentation is optional; never swallow an application's router error.
    const result = original.apply(this, args);
    safely("navigation dispatch", () => {
      if (window.location.href !== previousUrl)
        window.dispatchEvent(new Event(NAVIGATION_EVENT));
    });
    return result;
  };
}
