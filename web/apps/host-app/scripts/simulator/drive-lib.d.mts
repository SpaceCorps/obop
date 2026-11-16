export const ACTIONS: Readonly<Record<string, number>>;
export function scriptProblems(script: string): string[];
export function testArgs(options: {
  project: string;
  device: string;
  derivedData: string;
  resultBundle: string;
}): string[];
export function runnerEnv(
  env: Record<string, string | undefined>,
): Record<string, string | undefined>;
export function sessionVerdict(line: string): "passed" | "failed" | null;
export function redact(text: string, secrets: string[]): string;
