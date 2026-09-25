export default defineNuxtConfig({
  app: {
    head: {
      htmlAttrs: { lang: "en" },
      link: [
        {
          href: "/favicon.ico",
          rel: "icon",
          type: "image/x-icon",
          sizes: "16x16 32x32 48x48 64x64 128x128 256x256",
        },
        {
          rel: "apple-touch-icon",
          href: "/apple-touch-icon.png",
          sizes: "180x180",
        },
        { rel: "manifest", href: "/site.webmanifest" },
      ],
      meta: [
        { name: "msapplication-config", content: "/browserconfig.xml" },
        {
          content: "#f4f1e8",
          name: "theme-color",
        },
      ],
    },
  },
  compatibilityDate: "2026-05-16",
  // Keep the cascade explicit: shared primitives, management feature styles,
  // then breakpoint overrides. This mirrors the old monolithic source order.
  css: [
    "~/assets/css/main.css",
    "~/assets/css/management-features.css",
    "~/assets/css/responsive.css",
  ],
  ssr: false,
  nitro: { preset: "static" },
  devServer: {
    host: "0.0.0.0",
    port: 6173,
  },
  devtools: {
    enabled: process.env.NODE_ENV !== "production",
  },
  runtimeConfig: {
    public: {
      apiBase: "",
      privacyNoticeUrl: "",
      termsUrl: "",
    },
  },
  routeRules: {
    "/**": {
      headers: {
        "Cross-Origin-Opener-Policy": "same-origin",
        "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
        "X-Robots-Tag": "noindex, nofollow",
        "Cache-Control": "private, no-store",
        "Permissions-Policy":
          "camera=(), geolocation=(), microphone=(), payment=(), usb=()",
        "Referrer-Policy": "no-referrer",
        "X-Content-Type-Options": "nosniff",
        "X-Frame-Options": "DENY",
        "X-Permitted-Cross-Domain-Policies": "none",
      },
    },
  },
  typescript: {
    strict: true,
    tsConfig: {
      compilerOptions: {
        // Node 26 executes the dependency-free console tests via native type
        // stripping, which requires explicit `.ts` import extensions.
        allowImportingTsExtensions: true,
      },
    },
    typeCheck: true,
  },
  vite: {
    build: {
      chunkSizeWarningLimit: 650,
    },
  },
});
