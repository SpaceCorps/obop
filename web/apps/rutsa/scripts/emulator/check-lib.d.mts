export interface UiNode {
  id: string;
  text: string;
  desc: string;
  bounds: [number, number, number, number];
}
export function parseNodes(xml: string): UiNode[];
export function byId(nodes: UiNode[], id: string): UiNode | null;
export function byText(nodes: UiNode[], text: string): UiNode | null;
export function center(node: UiNode): { x: number; y: number };
export function parseKm(text: string): number | null;
export function judgeKm(
  shownKm: number,
  referenceM: number,
  tolerance: number,
): { ok: boolean; expectedKm: number; offKm: number; allowedKm: number };
