export interface Finding {
  level: "fail" | "warn" | "ok";
  text: string;
}

export function checkBuild(
  built: { config: unknown; infoPlist: unknown },
  options: { mode: "release" | "test"; drive?: boolean },
): { ok: boolean; findings: Finding[]; facts: Record<string, string> };
