import { useAnalytics } from "./index";
import { trackRules } from "./rules";
import { trackPerf } from "./performance";
import { readScriptConfig } from "./internal/config";
import { safely, reportFailure } from "./internal/safety";

// One self-contained payload. Reuse the module APIs and shared registry rather
// than downloading scripts or introducing a second controller lifecycle.
safely("full CDN initialization", () => {
  const { config, siteId } = readScriptConfig(
    document.currentScript as HTMLScriptElement | null,
  );
  if (!siteId) {
    reportFailure("missing data-owleye-id");
    return;
  }

  const analytics = useAnalytics(siteId, config);
  const rules = trackRules(siteId, config);
  const performanceTracking = trackPerf(siteId, config);
  performanceTracking.observeVitals();
  Object.assign(window, {
    OwlEyeAnalytics: analytics,
    OwlEyeRules: rules,
    OwlEyePerformance: performanceTracking,
  });
});
