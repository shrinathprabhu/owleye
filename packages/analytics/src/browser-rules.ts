import { trackRules } from "./rules";
import { readScriptConfig } from "./internal/config";

import { safely, reportFailure } from "./internal/safety";

safely("CDN initialization", () => {
  const { config, siteId } = readScriptConfig(
    document.currentScript as HTMLScriptElement | null,
  );

  if (siteId) {
    const tracking = trackRules(siteId, config);
    Object.assign(window, { OwlEyeRules: tracking });
  } else {
    reportFailure("missing data-owleye-id");
  }
});
