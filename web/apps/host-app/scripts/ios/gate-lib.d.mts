export interface Finding {
  level: "fail" | "warn" | "ok";
  text: string;
}

export interface Signing {
  profile: boolean;
  signature: "none" | "adhoc" | "identity";
  team: string | null;
}

export function signingFacts(text: string, profile: boolean): Signing;

export function checkBuild(
  built: { config: unknown; infoPlist: unknown; signing: Signing },
  options: { mode: "release" | "test" },
): { ok: boolean; findings: Finding[]; facts: Record<string, string> };
