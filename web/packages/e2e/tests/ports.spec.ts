// The ports of a run: one port for each server, all different (support/ports.ts). A server that finds its
// port taken stops with "Address already in use" and Playwright reports that it could not start.
import { createServer, type Server } from "node:net";
import { expect, test } from "@playwright/test";
import { PORT_VARIABLES, assignPorts, freePorts } from "../support/ports.ts";

const COUNT = PORT_VARIABLES.length;

function listen(port: number): Promise<Server> {
  return new Promise((ok, fail) => {
    const server = createServer();
    server.once("error", fail);
    server.listen(port, "127.0.0.1", () => ok(server));
  });
}

function close(server: Server): Promise<void> {
  return new Promise((done) => server.close(() => done()));
}

test.describe("the ports of a run", () => {
  test("the servers of this run each have a port of their own", () => {
    const ports = PORT_VARIABLES.map((name) => process.env[name]);
    expect(ports.every((port) => Number(port) > 0)).toBe(true);
    expect(new Set(ports).size).toBe(COUNT);
  });

  test("picking the ports of a run gives different ones, 200 times over", async () => {
    for (let round = 0; round < 200; round++) {
      const ports = await freePorts(COUNT);
      expect(new Set(ports).size, `round ${round}: ${ports.join(", ")}`).toBe(COUNT);
    }
  });

  test("the ports picked are free to bind, all at once, and nothing is left open", async () => {
    const ports = await freePorts(COUNT);
    const servers = await Promise.all(ports.map(listen));
    await Promise.all(servers.map(close));
    // And free again once released.
    await Promise.all((await Promise.all(ports.map(listen))).map(close));
  });

  test("a port that is already set is kept, the others are filled in", async () => {
    const env: NodeJS.ProcessEnv = { OBOP_E2E_APP_PORT: "20701" };
    await assignPorts(env);
    expect(env.OBOP_E2E_APP_PORT).toBe("20701");
    const ports = PORT_VARIABLES.map((name) => env[name]);
    expect(ports.every((port) => Number(port) > 0)).toBe(true);
    expect(new Set(ports).size).toBe(COUNT);
  });

  test("two servers given one port is an error that names both, not a failed start later", async () => {
    const env: NodeJS.ProcessEnv = Object.fromEntries(
      PORT_VARIABLES.map((name) => [name, "20702"]),
    );
    await expect(assignPorts(env)).rejects.toThrow(
      /OBOP_E2E_PLATFORM_PORT and OBOP_E2E_APP_PORT are both 20702/,
    );
  });
});
