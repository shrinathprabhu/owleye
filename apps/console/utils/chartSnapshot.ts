import type { ECharts, EChartsCoreOption } from "echarts/core";
import { isSnapshotPngDataUrl } from "./metricWidget.ts";

export type TrafficChartSnapshot = {
  svg: string;
  width: number;
  height: number;
};

/** Render the live chart option only on export; the on-screen renderer stays canvas. */
export async function chartSnapshot(
  chart: Pick<ECharts, "getOption" | "getWidth" | "getHeight">,
): Promise<TrafficChartSnapshot> {
  // Capture state before the lazy renderer loads; later navigation/selection cannot mix snapshots.
  const width = Math.round(chart.getWidth());
  const height = Math.round(chart.getHeight());
  if (
    !Number.isFinite(width) ||
    !Number.isFinite(height) ||
    width < 1 ||
    height < 1 ||
    width > 8192 ||
    height > 8192
  )
    throw new Error("Chart dimensions unavailable.");
  const option = chart.getOption();
  const [{ init, use }, { SVGRenderer }] = await Promise.all([
    import("echarts/core"),
    import("echarts/renderers"),
  ]);
  use([SVGRenderer]);
  const mirror = init(null, undefined, {
    renderer: "svg",
    ssr: true,
    width,
    height,
  });
  try {
    // Formatter functions are kept in memory, never serialized into the exported code.
    mirror.setOption({ ...option, animation: false } as EChartsCoreOption, {
      notMerge: true,
    });
    return {
      svg: await inlineBundledIcons(
        // Hover rules are irrelevant to a static capture and cannot enter embed code.
        mirror
          .renderToSVGString({ useViewBox: true })
          .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, ""),
      ),
      width,
      height,
    };
  } finally {
    mirror.dispose();
  }
}

// This is intentionally a fixed list of bundled files, never a referrer hostname or supplied URL.
const bundledIcons = new Set(
  [
    "bing.ico",
    "bluesky.svg",
    "brave.svg",
    "chatgpt.svg",
    "claude.ico",
    "cloudflare.png",
    "devto.png",
    "duckduckgo.ico",
    "facebook.ico",
    "figma.ico",
    "gemini.svg",
    "github.ico",
    "google.ico",
    "hackernews.ico",
    "indiehackers.png",
    "instagram.png",
    "linkedin.ico",
    "lowkey.svg",
    "medium.ico",
    "netlify.ico",
    "owleye.svg",
    "perplexity.ico",
    "producthunt.svg",
    "reddit.png",
    "stackoverflow.ico",
    "substack.ico",
    "threads.ico",
    "vercel.ico",
    "x.png",
    "yahoo.png",
    "youtube.ico",
  ].map((name) => `/referrer-icons/${name}`),
);

function rasterizeBundledIcon(path: string): Promise<string> {
  return new Promise((resolve, reject) => {
    if (typeof Image === "undefined" || typeof document === "undefined") {
      reject(new Error("Image export requires a browser."));
      return;
    }
    const image = new Image();
    const timer = setTimeout(() => finish(), 4000);
    function finish(value?: string) {
      clearTimeout(timer);
      image.onload = image.onerror = null;
      if (value && isSnapshotPngDataUrl(value)) resolve(value);
      else reject(new Error("Bundled source icon unavailable."));
    }
    image.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 32;
        const context = canvas.getContext("2d");
        if (!context || !image.naturalWidth || !image.naturalHeight) {
          finish();
          return;
        }
        const scale = Math.min(
          32 / image.naturalWidth,
          32 / image.naturalHeight,
        );
        const width = image.naturalWidth * scale;
        const height = image.naturalHeight * scale;
        context.drawImage(
          image,
          (32 - width) / 2,
          (32 - height) / 2,
          width,
          height,
        );
        finish(canvas.toDataURL("image/png"));
      } catch {
        finish();
      }
    };
    image.onerror = () => finish();
    image.referrerPolicy = "no-referrer";
    // Root-relative, allowlisted bundled path: no arbitrary or cross-origin image request.
    image.src = new URL(path, document.location.origin).href;
  });
}

async function inlineBundledIcons(svg: string): Promise<string> {
  const tags = [
    ...svg.matchAll(/<image\b[^>]*\/>|<image\b[^>]*>\s*<\/image>/gi),
  ];
  if (
    tags.length > 32 ||
    (svg.match(/<image\b/gi) ?? []).length !== tags.length
  )
    throw new Error("Chart source annotations could not be exported safely.");
  const icons = new Map<string, Promise<string | undefined>>();
  const replacements = await Promise.all(
    tags.map(async ([tag]) => {
      const href = /\s(?:xlink:)?href=(?:"([^"]*)"|'([^']*)')/i.exec(tag);
      const path = href?.[1] ?? href?.[2] ?? "";
      if (isSnapshotPngDataUrl(path)) return tag;
      if (!bundledIcons.has(path))
        throw new Error("Only bundled source icons can be exported.");
      if (!icons.has(path))
        icons.set(
          path,
          rasterizeBundledIcon(path).catch(() => undefined),
        );
      const png = await icons.get(path);
      if (png) return tag.replace(href![0], ` href="${png}"`);
      // A missing bundled asset should not prevent a useful static export. Keep its position,
      // using a neutral local vector badge, without retrying or requesting another origin.
      const transform = /\stransform="(matrix\([\d+.,eE\s-]+\))"/.exec(
        tag,
      )?.[1];
      return `<rect x="-1" y="-1" width="2" height="2" rx=".45" fill="#a391ff"${transform ? ` transform="${transform}"` : ""}/>`;
    }),
  );
  let index = 0;
  return svg.replace(
    /<image\b[^>]*\/>|<image\b[^>]*>\s*<\/image>/gi,
    () => replacements[index++]!,
  );
}
