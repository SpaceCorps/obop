/**
 * Planned prices of dedicated hosting (owner ruling OR-22, 2026-10-27): the provider's list price
 * of the server of the same size on 2026-10-27, times three, rounded up to the next price that ends
 * in 9. Extra storage follows the provider's block price times three. Euro, excluding VAT, per
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
    { vcpu: 1, ramGb: 1, storageGb: 20, providerEur: 9, eur: 29 },
    { vcpu: 2, ramGb: 2, storageGb: 20, providerEur: 17.8, eur: 59 },
    { vcpu: 3, ramGb: 4, storageGb: 20, providerEur: 28.8, eur: 89 },
    { vcpu: 4, ramGb: 8, storageGb: 20, providerEur: 45, eur: 139 },
    { vcpu: 8, ramGb: 16, storageGb: 20, providerEur: 88.2, eur: 269 },
    { vcpu: 16, ramGb: 32, storageGb: 20, providerEur: 224, eur: 679 },
  ],
  extraStorage: { blockGb: 50, providerEur: 5, eur: 15 },
};

/** The rule of the ruling, checked whenever the site is built: never below three times the provider. */
for (const s of HOSTING.servers) {
  if (s.eur < 3 * s.providerEur)
    throw new Error(`hosting price ${s.eur} is below three times ${s.providerEur}`);
}
if (HOSTING.extraStorage.eur < 3 * HOSTING.extraStorage.providerEur) {
  throw new Error("extra storage is priced below three times the provider's block price");
}
