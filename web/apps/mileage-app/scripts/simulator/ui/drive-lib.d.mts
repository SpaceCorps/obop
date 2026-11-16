export const TRIP_STEPS: string[];
export const ROUTE_MARKER: string;
export function testArgs(options: {
  project: string;
  scheme?: string;
  device: string;
  derivedData: string;
  resultBundle: string;
  only: string[];
}): string[];
export function runnerEnv(
  env: Record<string, string | undefined>,
): Record<string, string | undefined>;
export function isRouteMarker(line: string): boolean;
export function sessionVerdict(line: string): "passed" | "failed" | null;
export function redact(text: string, secrets: string[]): string;
