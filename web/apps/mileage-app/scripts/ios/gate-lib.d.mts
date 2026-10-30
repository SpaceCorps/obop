export interface Finding {
  level: "fail" | "warn" | "ok";
  text: string;
}

export function addressOf(text: string): string | undefined;

export function checkBuild(
  built: { config: unknown; infoPlist: unknown },
  options: {
    mode: "release" | "test";
    drive?: boolean;
    expectServer?: string;
    expectBundleId?: string;
  },
): { ok: boolean; findings: Finding[]; facts: Record<string, string> };
