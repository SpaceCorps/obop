import { PROBE_DEADLINE_MS, probeUrl, type ProbeOutcome } from "../core/probe.ts";

/**
 * One GET of the instance's API description with a deadline, without a token and without cookies.
 * Every failure (no route, a refused connection, a TLS failure, the deadline) is the same outcome:
 * no answer. What an answer means is decided by `classifyProbe`, not here.
 */
export async function probe(origin: string): Promise<ProbeOutcome> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), PROBE_DEADLINE_MS);
  try {
    const response = await fetch(probeUrl(origin), {
      method: "GET",
      headers: { Accept: "application/json" },
      credentials: "omit",
      cache: "no-store",
      signal: controller.signal,
    });
    const body = await response.text();
    return {
      answered: true,
      status: response.status,
      contentType: response.headers.get("content-type") ?? "",
      body,
      // The address after every redirect; an empty text when the system does not say.
      final: typeof response.url === "string" ? response.url : "",
    };
  } catch {
    return { answered: false };
  } finally {
    clearTimeout(timer);
  }
}
