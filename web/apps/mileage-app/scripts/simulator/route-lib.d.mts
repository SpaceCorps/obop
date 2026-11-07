export const DEFAULT_SPEED_MPS: number;

export function startArgs(options?: {
  device?: string;
  speed?: number;
  interval?: number;
  distance?: number;
}): string[];

export function routeTime(speed?: number): { metres: number; seconds: number };
