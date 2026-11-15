declare module "*?raw" {
  const text: string;
  export default text;
}

// `import.meta.glob`, for the lazy language catalogues and the registrations of extensions.ts (the
// app does not pull in vite/client).
interface ImportMeta {
  glob(pattern: string, options: { eager: true; import?: string }): Record<string, unknown>;
  glob(pattern: string, options?: { import?: string }): Record<string, () => Promise<unknown>>;
}
