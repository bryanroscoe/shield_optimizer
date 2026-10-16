import { error } from "@sveltejs/kit";
export function load() {
  if (!import.meta.env.DEV) error(404, "Not found");
  return {};
}
