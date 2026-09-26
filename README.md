# OwlEye

Privacy-preserving analytics on your own server. One Rust server serves the Nuxt Console at `/` and APIs at `/v1`. SQLite stores accounts and configuration; ClickHouse stores analytics. The monorepo contains only the API, Console, and analytics SDK.

[Website](https://owleye.dev) · [npm](https://www.npmjs.com/package/@owleye/analytics) · [Source](https://github.com/shrinathprabhu/owleye)

- [Self-hosting guide](docs/SELF_HOSTING.md): prerequisites, machine sizing, setup, redeploy, and update.
- [SDK usage guide](packages/analytics/README.md): npm and CDN setup, JavaScript globals, manual events, campaigns, rules, performance, and the 10-property contract.
- [Browser storage and visitor identification](packages/analytics/README.md#visitor-identification-and-browser-storage).
- [SDK release checklist](docs/SELF_HOSTING.md#publishing-the-sdk).

```sh
./setup.sh       # first installation and administrator account
./redeploy.sh    # build and restart using existing configuration
./update.sh      # git pull --ff-only, build, and restart
```

Recommended minimum: **4 vCPU, 8 GB RAM, and 20 GB free storage**. This is guidance, not an enforced restriction. Install Node.js 26.2+, pnpm 10.28.1, Rust 1.95+ with Cargo, ClickHouse, and SQLite before setup. Caddy is optional. Follow the [prerequisite installation links and commands](docs/SELF_HOSTING.md#installing-prerequisites), then run `./setup.sh --check`. Running setup again refuses to overwrite an existing installation; use redeploy to rebuild and restart.

Licensed under [MIT](LICENSE).

Maintained by **Shrinath** · [hello@shrinath.me](mailto:hello@shrinath.me) · [owleye.dev](https://owleye.dev).

Third-party dependencies and bundled assets retain their respective licenses,
including the [amCharts map data](apps/console/public/maps/LICENSE.amcharts).
