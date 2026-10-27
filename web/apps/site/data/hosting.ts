/**
 * Planned prices of dedicated hosting (owner ruling OR-22, 2026-10-27): the provider's list price
 * of the server of the same size on 2026-10-27, times two, rounded up to the next price that ends
 * in 9. Extra storage follows the provider's block price times two. Euro, excluding VAT, per
 * month. Nothing is offered or charged yet.
 *
 * Change the numbers here and nowhere else: every language, the home page and the Markdown
 * mirrors read them from this file. The one exception is the English FAQ page
 * (content/en/docs/reference/faq.md), which repeats the first server's price and the storage
 * price in words.
 */
export interface HostedServer {
  vcpu: number;
  ramGb: number;
  /** NVMe storage included. */
  storageGb: number;
  /** The provider's list price of this size on 2026-10-27, for the check below. */
  providerEur: number;
  /** What we plan to charge. */
  eur: number;
}

export const HOSTING: {
  servers: HostedServer[];
  extraStorage: { blockGb: number; providerEur: number; eur: number };
} = {
  servers: [
    { vcpu: 1, ramGb: 1, storageGb: 20, providerEur: 9, eur: 19 },
    { vcpu: 2, ramGb: 2, storageGb: 20, providerEur: 17.8, eur: 39 },
    { vcpu: 3, ramGb: 4, storageGb: 20, providerEur: 28.8, eur: 59 },
    { vcpu: 4, ramGb: 8, storageGb: 20, providerEur: 45, eur: 99 },
    { vcpu: 8, ramGb: 16, storageGb: 20, providerEur: 88.2, eur: 179 },
    { vcpu: 16, ramGb: 32, storageGb: 20, providerEur: 224, eur: 449 },
  ],
  extraStorage: { blockGb: 50, providerEur: 5, eur: 10 },
};

/** The rule of the ruling, checked whenever the site is built: never below twice the provider. */
for (const s of HOSTING.servers) {
  if (s.eur < 2 * s.providerEur)
    throw new Error(`hosting price ${s.eur} is below twice ${s.providerEur}`);
}
if (HOSTING.extraStorage.eur < 2 * HOSTING.extraStorage.providerEur) {
  throw new Error("extra storage is priced below twice the provider's block price");
}
