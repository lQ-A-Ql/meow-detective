export function formatStegScore(value?: number): string {
  if (value === undefined || !Number.isFinite(value)) return '-';
  return `${value.toFixed(1)} / 100`;
}

export function formatStegRatio(value?: number): string {
  if (value === undefined || !Number.isFinite(value)) return '-';
  return `${(value * 100).toFixed(1)}%`;
}
