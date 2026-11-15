export interface Finding {
  level: "fail" | "warn" | "ok";
  text: string;
}

export interface ManifestFacts {
  packageName: string;
  versionName: string;
  versionCode: string;
  usesCleartextTraffic: string | undefined;
  networkSecurityConfig: string | undefined;
  debuggable: string | undefined;
  allowBackup: string | undefined;
  hasApplication: boolean;
}

export function manifestFacts(xml: string): ManifestFacts;

export function permissionList(text: string): string[];

export function expectedPermissions(packageName: string): string[];

export function checkBuild(
  built: { manifest: string; permissions: string; config: unknown },
  options: { mode: "release" | "test" },
): { ok: boolean; findings: Finding[]; facts: Record<string, string> };
