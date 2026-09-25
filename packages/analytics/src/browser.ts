import { useAnalytics } from "./index";
import { readScriptConfig } from "./internal/config";

import { safely, reportFailure } from "./internal/safety";

safely("CDN initialization", () => {
  const { config, siteId } = readScriptConfig(
    document.currentScript as HTMLScriptElement | null,
  );

  if (siteId) {
    const analytics = useAnalytics(siteId, config);
    Object.assign(window, { OwlEyeAnalytics: analytics });
  } else {
    reportFailure("missing data-owleye-id");
  }
});
