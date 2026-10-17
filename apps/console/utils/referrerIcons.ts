type Provider = {
  file: string;
  labels: readonly string[];
  hosts: readonly string[];
};

// Fixed local assets only. Never turn analytics labels into network destinations.
const providers: readonly Provider[] = [
  {
    file: "gemini.svg",
    labels: ["Gemini", "Google Gemini"],
    hosts: ["gemini.google.com"],
  },
  {
    file: "google.ico",
    labels: ["Google", "Google Search"],
    hosts: [
      "google.com",
      "google.co.in",
      "google.co.uk",
      "google.co.jp",
      "google.com.au",
      "google.com.br",
      "google.de",
      "google.fr",
      "google.ca",
      "google.es",
      "google.it",
      "google.nl",
    ],
  },
  { file: "bing.ico", labels: ["Bing"], hosts: ["bing.com"] },
  { file: "duckduckgo.ico", labels: ["DuckDuckGo"], hosts: ["duckduckgo.com"] },
  { file: "yahoo.png", labels: ["Yahoo"], hosts: ["yahoo.com"] },
  { file: "brave.svg", labels: ["Brave Search"], hosts: ["search.brave.com"] },
  {
    file: "chatgpt.svg",
    labels: ["ChatGPT", "OpenAI"],
    hosts: ["chatgpt.com", "chat.openai.com"],
  },
  { file: "claude.ico", labels: ["Claude"], hosts: ["claude.ai"] },
  { file: "perplexity.ico", labels: ["Perplexity"], hosts: ["perplexity.ai"] },
  {
    file: "x.png",
    labels: ["X", "Twitter"],
    hosts: ["x.com", "twitter.com", "t.co"],
  },
  { file: "github.ico", labels: ["GitHub"], hosts: ["github.com"] },
  {
    file: "linkedin.ico",
    labels: ["LinkedIn"],
    hosts: ["linkedin.com", "lnkd.in"],
  },
  { file: "reddit.png", labels: ["Reddit"], hosts: ["reddit.com", "redd.it"] },
  {
    file: "producthunt.svg",
    labels: ["Product Hunt", "ProductHunt"],
    hosts: ["producthunt.com"],
  },
  {
    file: "hackernews.ico",
    labels: ["Hacker News", "HackerNews", "Y Combinator"],
    hosts: ["news.ycombinator.com", "ycombinator.com"],
  },
  {
    file: "youtube.ico",
    labels: ["YouTube"],
    hosts: ["youtube.com", "youtu.be"],
  },
  {
    file: "facebook.ico",
    labels: ["Facebook"],
    hosts: ["facebook.com", "fb.com", "fb.me"],
  },
  { file: "instagram.png", labels: ["Instagram"], hosts: ["instagram.com"] },
  {
    file: "threads.ico",
    labels: ["Threads"],
    hosts: ["threads.com", "threads.net"],
  },
  { file: "bluesky.svg", labels: ["Bluesky"], hosts: ["bsky.app"] },
  {
    file: "stackoverflow.ico",
    labels: ["Stack Overflow"],
    hosts: ["stackoverflow.com"],
  },
  { file: "medium.ico", labels: ["Medium"], hosts: ["medium.com"] },
  { file: "substack.ico", labels: ["Substack"], hosts: ["substack.com"] },
  { file: "devto.png", labels: ["DEV Community", "DEV"], hosts: ["dev.to"] },
  {
    file: "indiehackers.png",
    labels: ["Indie Hackers"],
    hosts: ["indiehackers.com"],
  },
  { file: "vercel.ico", labels: ["Vercel"], hosts: ["vercel.com"] },
  { file: "netlify.ico", labels: ["Netlify"], hosts: ["netlify.com"] },
  { file: "cloudflare.png", labels: ["Cloudflare"], hosts: ["cloudflare.com"] },
  { file: "figma.ico", labels: ["Figma"], hosts: ["figma.com"] },
  {
    file: "lowkey.svg",
    labels: ["lowkey.tools", "lowkey tools"],
    hosts: ["lowkey.tools"],
  },
  { file: "owleye.svg", labels: ["OwlEye"], hosts: ["owleye.dev"] },
];

const byLabel = new Map(
  providers.flatMap((provider) =>
    provider.labels.map((label) => [label.toLowerCase(), provider] as const),
  ),
);
const byHost = providers
  .flatMap((provider) => provider.hosts.map((host) => ({ host, provider })))
  .sort((a, b) => b.host.length - a.host.length);

function providerFor(value: string | undefined): Provider | undefined {
  if (!value || value.length > 512 || /[\u0000-\u001f\u007f]/u.test(value))
    return undefined;
  const normalized = value.trim().toLowerCase();
  const label = byLabel.get(normalized);
  if (label) return label;
  const host = normalized.replace(/\.$/, "");
  if (!/^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/.test(host))
    return undefined;
  return byHost.find(
    (entry) => host === entry.host || host.endsWith(`.${entry.host}`),
  )?.provider;
}

/** Same-origin bundled image, or undefined for the themed globe fallback. */
export function referrerIconSource(
  value: string | undefined,
): string | undefined {
  const provider = providerFor(value);
  return provider ? `/referrer-icons/${provider.file}` : undefined;
}

/** A fixed recognized public host; never an arbitrary analytics-supplied URL. */
export function referrerIconHost(
  value: string | undefined,
): string | undefined {
  return providerFor(value)?.hosts[0];
}
