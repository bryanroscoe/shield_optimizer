// The brand picked in the connection help. Both desktop disclosures (Connection
// help and Pair PIN's Help with pairing) show it, so they share one value.
import { savedBrand, saveBrand } from "../../shared/connection-setup";

function initial(): string {
  try {
    return typeof localStorage === "undefined" ? "" : (savedBrand(localStorage) ?? "");
  } catch {
    return "";
  }
}

export const connectionBrand = $state({ id: initial() });

export function chooseConnectionBrand(id: string): void {
  connectionBrand.id = id;
  try {
    saveBrand(localStorage, id || null);
  } catch {
    /* storage unavailable: the pick lasts until the app closes */
  }
}
