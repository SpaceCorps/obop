export type Call = (path: string, payload: unknown) => Promise<any>;
export function connect(who: { server: string; user: string; password: string }): Promise<Call>;
export function findTrip(call: Call, which: { tripId?: string; driver?: string }): Promise<any>;
export function tripPoints(call: Call, tripId: string): Promise<any[]>;
