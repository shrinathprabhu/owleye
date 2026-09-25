export function trackingSetupSnippet(siteId: string, apiBase: string) {
  const server =
    apiBase.trim() ||
    (typeof window !== "undefined"
      ? window.location.origin
      : "https://analytics.example.com");
  const options = `, {\n  server: ${JSON.stringify(server)},\n}`;

  return `import { useAnalytics } from "@owleye/analytics";

const analytics = useAnalytics(${JSON.stringify(siteId)}${options});

analytics.track("signup_completed", { plan: "starter" });`;
}
