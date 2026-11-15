export interface Bounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface UiNode {
  id: string;
  text: string;
  description: string;
  className: string;
  enabled: boolean;
  bounds: Bounds;
}

export function parseUiDump(xml: string): UiNode[];

export function nodeById(nodes: UiNode[], id: string): UiNode | undefined;

export function hasText(nodes: UiNode[], part: string): boolean;

export function centre(bounds: Bounds): { x: number; y: number };

export function devtoolsSockets(procNetUnix: string): { name: string; pid: number }[];

export function resumedPackage(dumpsys: string): string | undefined;

export function homePackage(output: string): string | undefined;

export function inputTextArgument(text: string): string;

export function parseAccount(text: string): { user: string; password: string } | undefined;

export function instancePage(
  targets: unknown,
  origin: string,
): { url: string; webSocketDebuggerUrl: string } | undefined;
