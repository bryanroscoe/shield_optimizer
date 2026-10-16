import { error } from "@sveltejs/kit";
import { exitPreviewEnabled } from "$lib/exit-preview";

export function load() {
  if (!exitPreviewEnabled) error(404, "Not found");
  return {};
}
