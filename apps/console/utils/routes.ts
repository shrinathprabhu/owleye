const segment = (value: string) => encodeURIComponent(value);
const site = (siteId: string) => `/v1/sites/${segment(siteId)}`;
const dashboard = (siteId: string, dashboardId: string) =>
  `${site(siteId)}/dashboards/${segment(dashboardId)}`;
const dashboardInvite = (siteId: string, dashboardId: string) =>
  `${site(siteId)}/dashboard-invites/${segment(dashboardId)}`;
const dashboardWidget = (
  siteId: string,
  dashboardId: string,
  widgetId: string,
) => `${dashboard(siteId, dashboardId)}/widgets/${segment(widgetId)}`;

export const routes = {
  account: {
    profile: "/v1/account/profile",
  },
  auth: {
    logout: "/v1/auth/logout",
    onboardingSkipTwoFactor: "/v1/auth/onboarding/2fa/skip",
    refresh: "/v1/auth/refresh",
    session: "/v1/auth/session",
    twoFactorEnable: "/v1/auth/2fa/enable",
    twoFactorSetup: "/v1/auth/2fa/setup",
    twoFactorVerify: "/v1/auth/2fa/verify",
  },
  health: "/health",
  sites: {
    live: (siteId: string) => `${site(siteId)}/live`,
    uptime: (siteId: string) => `${site(siteId)}/uptime`,
    uptimeMonitor: (siteId: string, monitorId: string) =>
      `${site(siteId)}/uptime/${segment(monitorId)}`,
    uptimeHistory: (siteId: string, monitorId: string) =>
      `${site(siteId)}/uptime/${segment(monitorId)}/history`,
    ai: (siteId: string) => `${site(siteId)}/ai`,
    apiKeys: (siteId: string) => `${site(siteId)}/api-keys`,
    apiKey: (siteId: string, keyId: string) =>
      `${site(siteId)}/api-keys/${segment(keyId)}`,
    campaigns: (siteId: string) => `${site(siteId)}/campaigns`,
    campaign: (siteId: string, campaignId: string) =>
      `${site(siteId)}/campaigns/${segment(campaignId)}`,
    dashboard,
    dashboardInvite,
    dashboardInviteDecision: (
      siteId: string,
      dashboardId: string,
      decision: "accept" | "dismiss",
    ) => `${dashboardInvite(siteId, dashboardId)}/${decision}`,
    dashboardInvites: (siteId: string) => `${site(siteId)}/dashboard-invites`,
    dashboardPreview: (siteId: string) => `${site(siteId)}/dashboards/preview`,
    dashboards: (siteId: string) => `${site(siteId)}/dashboards`,
    dashboardShares: (siteId: string, dashboardId: string) =>
      `${dashboard(siteId, dashboardId)}/shares`,
    dashboardShare: (siteId: string, dashboardId: string, userId: string) =>
      `${dashboard(siteId, dashboardId)}/shares/${segment(userId)}`,
    dashboardShareSelf: (siteId: string, dashboardId: string) =>
      `${dashboard(siteId, dashboardId)}/shares/me`,
    dashboardWidget,
    dashboardWidgets: (siteId: string, dashboardId: string) =>
      `${dashboard(siteId, dashboardId)}/widgets`,
    dashboardWidgetPreview: (siteId: string, dashboardId: string) =>
      `${dashboard(siteId, dashboardId)}/widgets/preview`,
    developer: (siteId: string) => `${site(siteId)}/developer`,
    developerKeys: (siteId: string) => `${site(siteId)}/developer/keys`,
    developerKey: (siteId: string, keyId: string) =>
      `${site(siteId)}/developer/keys/${segment(keyId)}`,
    domains: (siteId: string) => `${site(siteId)}/domains`,
    domain: (siteId: string, domainId: string) =>
      `${site(siteId)}/domains/${segment(domainId)}`,
    events: (siteId: string) => `${site(siteId)}/events`,
    list: "/v1/sites",
    members: (siteId: string) => `${site(siteId)}/members`,
    member: (siteId: string, userId: string) =>
      `${site(siteId)}/members/${segment(userId)}`,
    prompt: (siteId: string) => `${site(siteId)}/prompt`,
    performance: (siteId: string) => `${site(siteId)}/performance`,
    rules: (siteId: string) => `${site(siteId)}/rules`,
    rule: (siteId: string, ruleId: string) =>
      `${site(siteId)}/rules/${segment(ruleId)}`,
    settings: (siteId: string) => `${site(siteId)}/settings`,
    support: (siteId: string) => `${site(siteId)}/support`,
    one: site,
  },
} as const;
