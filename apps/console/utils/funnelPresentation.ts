/** Geometry follows the server's released share; it never infers hidden stages. */
export function funnelBarWidth(value: number) {
  return Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : 0;
}

export function funnelPercent(value: number) {
  return Number.isFinite(value)
    ? `${value.toLocaleString(undefined, { maximumFractionDigits: 1 })}%`
    : "Unavailable";
}
