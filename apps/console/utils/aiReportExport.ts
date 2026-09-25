import type { AiPromptResponse, AiMetric } from "~/types/ai";
import {
  metricLabels,
  reportLabel,
  reportNotes,
  reportScope,
} from "./aiReport";
import type { Content, TDocumentDefinitions } from "pdfmake/interfaces";
export type ChartSnapshot = { data: string; width: number; height: number };
export async function exportAiReport(
  question: string,
  response: AiPromptResponse,
  metric: AiMetric,
  chart?: ChartSnapshot,
) {
  const e = response.evidence;
  const title = "OwlEye analytics report";
  const subtitle = `${e.start_date} – ${e.end_date} UTC · ${reportScope(e)}`;
  const notes = reportNotes(e);
  const funnel = e.report === "funnel";
  const table = funnel
    ? [
        ["Stage", "Anonymous visitors"],
        ...e.rows.map((row) => [
          row.label,
          row.visitors.toLocaleString("en-US"),
        ]),
      ]
    : [
        ["Group", "Events", "Pageviews", "Visitors", "Sessions"],
        ...e.rows.map((row) => [
          reportLabel(row.label, e.report),
          ...[row.events, row.pageviews, row.visitors, row.sessions].map((n) =>
            n.toLocaleString("en-US"),
          ),
        ]),
      ];
  const filename = `owleye-${e.report}-${e.start_date}-${e.end_date}.pdf`;
  const [{ default: pdfMake }, { default: fonts }] = await Promise.all([
    import("pdfmake/build/pdfmake"),
    import("pdfmake/build/vfs_fonts"),
  ]);
  pdfMake.addVirtualFileSystem(fonts);
  const content: Content[] = [
    { text: title, style: "title" },
    { text: subtitle, margin: [0, 0, 0, 14] },
    { text: question, style: "heading" },
    { text: response.answer, margin: [0, 0, 0, 16] },
  ];
  if (chart)
    content.push(
      { text: metricLabels[metric], style: "heading" },
      { image: chart.data, fit: [480, 300], margin: [0, 0, 0, 16] },
    );
  content.push({ text: "Supporting data", style: "heading" });
  if (e.rows.length)
    content.push({
      table: {
        headerRows: 1,
        widths: funnel ? ["*", "auto"] : ["*", "auto", "auto", "auto", "auto"],
        body: table,
      },
      layout: "lightHorizontalLines",
      fontSize: 9,
    });
  else content.push({ text: "No matching report rows." });
  content.push(
    { text: "About this report", style: "heading" },
    ...notes.map((text) => ({ text, margin: [0, 0, 0, 6] }) as Content),
  );
  const doc: TDocumentDefinitions = {
    pageSize: "A4",
    pageMargins: [40, 44, 40, 44],
    content,
    defaultStyle: { font: "Roboto", fontSize: 10, lineHeight: 1.25 },
    styles: {
      title: { fontSize: 22, bold: true, margin: [0, 0, 0, 10] },
      heading: { fontSize: 12, bold: true, margin: [0, 12, 0, 8] },
    },
    footer: (page, count) => ({
      text: `OwlEye · ${page} / ${count}`,
      alignment: "center",
      fontSize: 8,
      color: "#666666",
    }),
  };
  const blob = await pdfMake.createPdf(doc).getBlob();
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
