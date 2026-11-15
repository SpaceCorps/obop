/**
 * Loads every registration of `src/registrations/` (see extensions.ts). Imported by the shell, the
 * record panel and the tests that render them; a new registration file needs no line here.
 */
import.meta.glob("./registrations/*.{ts,tsx}", { eager: true });
