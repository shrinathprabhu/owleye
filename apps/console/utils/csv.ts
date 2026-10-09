export type CsvCell = string | number | null | undefined;

/** Quote every cell and neutralize spreadsheet formulas in untrusted labels. */
export function csvText(rows: CsvCell[][]): string {
  return (
    rows
      .map((row) =>
        row
          .map((value) => {
            let text = value == null ? "" : String(value);
            if (
              typeof value === "string" &&
              /^[\s\u0000-\u001f]*[=+@-]/.test(text)
            )
              text = `'${text}`;
            return `"${text.replaceAll('"', '""')}"`;
          })
          .join(","),
      )
      .join("\r\n") + "\r\n"
  );
}

export function downloadCsv(filename: string, rows: CsvCell[][]) {
  const url = URL.createObjectURL(
    new Blob(["\uFEFF", csvText(rows)], { type: "text/csv;charset=utf-8" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
