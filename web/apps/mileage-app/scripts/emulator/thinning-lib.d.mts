export interface Thinning {
  minGapMs: number;
  farM: number;
}
export interface TimedFix {
  t: number;
  lat: number;
  lon: number;
}
export const RULES: { name: string; thinning: Thinning | null }[];
export function toFixes(rows: Array<Record<string, unknown>>): TimedFix[];
export function dropFirst(fixes: TimedFix[], n: number): TimedFix[];
export function percent(fraction: number): string;
export function replay(
  fixes: TimedFix[],
  thinning: Thinning | null,
): { kept: number; distanceM: number };
export function cadence(fixes: Array<{ t: number }>): {
  count: number;
  spanS: number;
  perMinute: number;
  gapsS: { smallest: number; median: number; largest: number } | null;
};
export function looksThinned(fixes: Array<{ t: number }>): boolean;
