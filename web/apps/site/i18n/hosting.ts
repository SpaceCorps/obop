import { HOSTING } from "../data/hosting.ts";
import type { Plan } from "./types.ts";

export interface HostingWords {
  /** A euro amount as the language writes it, for example "€19" or "19 €". */
  eur: (amount: number) => string;
  /** The unit of memory and storage: "GB", or "Go" in French. */
  unit: string;
  /** "/month". */
  period: string;
  /** The free self-hosted card, which comes first. */
  selfHost: Plan;
  /** What every hosted server card says in one sentence. */
  serverBlurb: string;
  /** The ticks of a hosted server card, given the storage that is included. */
  serverFeatures: (storageGb: number) => string[];
}

/** The self-hosted card and one card per planned hosted server, prices from `data/hosting.ts`. */
export function hostedPlans(w: HostingWords): Plan[] {
  return [
    w.selfHost,
    ...HOSTING.servers.map((s) => ({
      name: `${s.vcpu} vCPU · ${s.ramGb} ${w.unit} RAM`,
      price: w.eur(s.eur),
      period: w.period,
      blurb: w.serverBlurb,
      features: w.serverFeatures(s.storageGb),
    })),
  ];
}
