import { trackPerf } from "./performance";
import { readScriptConfig } from "./internal/config";

import { safely, reportFailure } from "./internal/safety";

safely("CDN initialization", () => {
  const { config, siteId } = readScriptConfig(
    document.currentScript as HTMLScriptElement | null,
  );

  if (siteId) {
    const performanceTracking = trackPerf(siteId, config);
    performanceTracking.observeVitals();
    Object.assign(window, { OwlEyePerformance: performanceTracking });
  } else {
    reportFailure("missing data-owleye-id");
  }
});
