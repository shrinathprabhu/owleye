/** Stable, non-empty chart selection. Never mutates the caller's selection. */
export function toggleChartMetric<T extends string>(
  selected: readonly T[],
  metric: T,
  allowed: readonly T[],
): T[] {
  const valid = allowed.filter((key) => selected.includes(key));
  if (!allowed.includes(metric)) return valid.length ? valid : [...allowed];
  if (valid.includes(metric))
    return valid.length === 1 ? valid : valid.filter((key) => key !== metric);
  return allowed.filter((key) => key === metric || valid.includes(key));
}
