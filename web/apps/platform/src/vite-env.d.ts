// `import.meta.glob`, for the lazy language catalogues (the app does not pull in vite/client).
interface ImportMeta {
  glob(pattern: string, options?: { import?: string }): Record<string, () => Promise<unknown>>;
}
