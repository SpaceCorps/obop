export interface SimulatorBuild {
  workspace: string;
  scheme: string;
  configuration?: string;
  archs?: string;
  derivedData: string;
}

export function simulatorBuildArgs(options: SimulatorBuild): string[];
export function simulatorAppPath(
  options: Pick<SimulatorBuild, "scheme" | "configuration" | "derivedData">,
): string;
