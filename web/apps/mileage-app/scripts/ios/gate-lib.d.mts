export interface Finding {
  level: "fail" | "warn" | "ok";
  text: string;
}

export function checkBuild(
  built: { config: unknown; infoPlist: unknown },
  options: { mode: "release" | "test" },
): { ok: boolean; findings: Finding[]; facts: Record<string, string> };
