<script setup lang="ts">
import { metricWidgetSvg } from "~/utils/metricWidget";
const props = defineProps<{
  label: string;
  value?: number;
  period: string;
  site: string;
  disabled?: boolean;
}>();
const busy = ref(false);
const notice = ref("");
function download(blob: Blob, extension: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `owleye-snapshot.${extension}`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
async function exportSnapshot(format: "svg" | "png" | "code") {
  if (props.value === undefined || props.disabled || busy.value) return;
  busy.value = true;
  notice.value = "";
  try {
    const svg = metricWidgetSvg({ ...props, value: props.value });
    if (format === "code") {
      await navigator.clipboard.writeText(svg);
      notice.value = "Static SVG code copied. It makes no API requests.";
    } else if (format === "svg") {
      download(new Blob([svg], { type: "image/svg+xml" }), "svg");
      notice.value = "SVG snapshot exported.";
    } else {
      const url = URL.createObjectURL(
        new Blob([svg], { type: "image/svg+xml" }),
      );
      try {
        const image = new Image();
        image.src = url;
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = 1440;
        canvas.height = 720;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Canvas unavailable");
        context.drawImage(image, 0, 0, canvas.width, canvas.height);
        const blob = await new Promise<Blob | null>((resolve) =>
          canvas.toBlob(resolve, "image/png"),
        );
        if (!blob) throw new Error("Image export unavailable");
        download(blob, "png");
        notice.value = "PNG snapshot exported.";
      } finally {
        URL.revokeObjectURL(url);
      }
    }
  } catch {
    notice.value =
      "Export failed. Try SVG download, or allow clipboard access for code.";
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <details class="snapshot-export">
    <summary>Export selected metric as a widget</summary>
    <p>
      Share {{ label.toLowerCase() }} for {{ site }} and this date range. PNG,
      SVG and embed code are static snapshots; only export data you intend to
      share.
    </p>
    <div class="export-actions">
      <button
        v-for="format in ['png', 'svg', 'code'] as const"
        :key="format"
        type="button"
        class="button secondary compact"
        :disabled="disabled || value === undefined || busy"
        @click="exportSnapshot(format)"
      >
        {{
          format === "code"
            ? "Copy embed code"
            : `Download ${format.toUpperCase()}`
        }}
      </button>
    </div>
    <p role="status">{{ notice }}</p>
  </details>
</template>
<style scoped>
.snapshot-export {
  border: 1px solid var(--border-subtle);
  border-radius: 12px;
  background: var(--bg-elevated);
  padding: 14px 18px;
  margin: -8px 0 24px;
}
summary {
  font-weight: 650;
  cursor: pointer;
  color: var(--text-primary);
}
p {
  font-size: 0.8rem;
  line-height: 1.6;
  color: var(--text-secondary);
}
.export-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}
</style>
