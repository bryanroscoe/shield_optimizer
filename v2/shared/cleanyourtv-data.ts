// The bundled CleanYourTV catalog and every list's original text, in one
// module that `cleanyourtv-source.ts` loads only when a screen asks for it.

import catalog from "../catalogs/cleanyourtv/catalog.json";
import type { ProfileCatalog } from "./profile-catalog";

export const profileCatalog = catalog as unknown as ProfileCatalog;

export const sourceTexts = import.meta.glob<string>("../catalogs/cleanyourtv/upstream/examples/*/debloat.txt", {
  query: "?raw",
  import: "default",
  eager: true,
});
