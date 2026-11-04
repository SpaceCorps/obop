// Free loopback ports for the servers of one run.
import { createServer, type AddressInfo, type Server } from "node:net";

/** The environment variables that carry the ports of a run, in the order they are picked. */
export const PORT_VARIABLES = [
  "OBOP_E2E_APP_PORT",
  "OBOP_E2E_PLATFORM_PORT",
  "OBOP_E2E_SHOP_PORT",
  "OBOP_E2E_MARKET_PORT",
  "OBOP_E2E_PASSKEY_PORT",
  "OBOP_E2E_LANGUAGE_PORT",
  "OBOP_E2E_SSO_PORT",
  "OBOP_E2E_IDP_PORT",
] as const;

/**
 * Picks `count` distinct free loopback ports.
 *
 * Every probe socket stays open until all the ports are picked, and they are closed together at the
 * end. Closing each one before the next is chosen lets the operating system hand the same port out a
 * second time, and two servers of the run then fight for it ("Address already in use").
 */
export async function freePorts(count: number): Promise<number[]> {
  const probes: Server[] = [];
  try {
    const ports: number[] = [];
    for (let i = 0; i < count; i++) {
      const probe = createServer();
      probes.push(probe);
      ports.push(
        await new Promise<number>((ok, fail) => {
          probe.once("error", fail);
          probe.listen(0, "127.0.0.1", () => ok((probe.address() as AddressInfo).port));
        }),
      );
    }
    return ports;
  } finally {
    await Promise.all(
      probes.map((probe) => new Promise<void>((done) => probe.close(() => done()))),
    );
  }
}

/**
 * Gives every variable in PORT_VARIABLES a port, keeping one that is already set (the workers a
 * runner spawns inherit the runner's choice). Throws if two of the eight end up equal.
 */
export async function assignPorts(env: NodeJS.ProcessEnv = process.env): Promise<void> {
  const missing = PORT_VARIABLES.filter((name) => !env[name]);
  const ports = await freePorts(missing.length);
  missing.forEach((name, i) => (env[name] = String(ports[i])));
  const seen = new Map<string, string>();
  for (const name of PORT_VARIABLES) {
    const port = env[name] as string;
    const other = seen.get(port);
    if (other)
      throw new Error(`${name} and ${other} are both ${port}; two servers cannot share a port`);
    seen.set(port, name);
  }
}
