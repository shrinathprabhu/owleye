# UA parser rules

Source: https://github.com/ua-parser/uap-core/tree/73e7340c3ed8055051607b296bf46ead7aa5f19e

`regexes.json` is a format-only conversion of `regexes.yaml` from that revision, fetched 2026-10-06. Apache-2.0; see LICENSE. Rules are embedded at compile time. There is no runtime download or remote code execution. Update deliberately with the source revision and parser fixture tests.

The Rust ua-parser extractor handles browser and OS families; Woothee remains a fallback for coarse device categories and crawler detection. Neither parser can identify a browser that deliberately hides its identity.

OwlEye supplements these unchanged upstream rules with explicit product-token recognition in `privacy/user_agent.rs`. Browser aliases are canonicalized in ingestion and reporting. The SDK also reads low-entropy browser brands and explicit UA tokens; OS and device classification remain API-side. Shared synthetic fixtures live in `packages/analytics/test/browser-fixtures.json` and exercise both parsers, including forks that expose only a base-engine identity.

Arc (including the ArcMobile2/Arc Search token), Comet, Dia, Zen, Floorp, Brave, Samsung Internet, Safari, Chrome, Edge, Firefox, Vivaldi, Opera, and UC Browser have canonical labels. Headless Chrome and Headless Edge remain separate from their ordinary browser families. Brand/token support is conditional: it does not mean every release of these browsers exposes that identity. When only Chrome/Firefox is exposed, that is the reported family; no CSS, extension, or fingerprinting probes are used to guess a hidden browser. Previously stored Chrome/Firefox rows cannot be retroactively separated into hidden forks.

Detection references: [Brave's documented signals](https://github.com/brave/brave-browser/wiki/Detecting-Brave-%28for-Websites%29), [Floorp's Firefox identity](https://github.com/Floorp-Projects/Floorp/discussions/1512), and [Zen release notes](https://zen-browser.app/release-notes/latest/). The token fixtures are regression inputs, not captures or claims of verification in installed browsers.
