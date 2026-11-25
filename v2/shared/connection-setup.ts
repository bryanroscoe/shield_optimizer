// Brand-specific steps for turning on debugging, from the owners' notes in
// CleanYourTV (MIT; see catalogs/connection-setup/NOTICE.md). The data says
// only what a source wrote down: a step it doesn't give falls back to the
// general steps, never to a guess.

import data from "../catalogs/connection-setup/connection-setup.json";

/// How the TV is reached once debugging is on: plain ADB on port 5555, a
/// pairing code (Wireless debugging), or either depending on the unit.
export type ConnectMethod = "port" | "pair" | "either";

interface MenuStep {
  path: string[];
  action: string;
  when?: string;
}

interface Toggle {
  name: string;
  then?: string;
  note?: string;
}

interface Variant {
  models: string;
  unlock: MenuStep | null;
  generic_unlock?: boolean;
  developer_options: string[] | null;
  turn_on: Toggle[];
  connect: ConnectMethod;
  notes: string[];
  sources: string[];
}

interface Brand {
  id: string;
  name: string;
  variants: Variant[];
}

interface SetupData {
  schema: number;
  source: { name: string; repo: string; commit: string; license: string };
  generic: { unlock: MenuStep[]; developer_options: string; turn_on: string; sources: string[] };
  brands: Brand[];
}

const setup = data as SetupData;

export const SEPARATOR = " › ";

/// One model group's steps, ready to show. The app adds its own last step
/// (which button to use next) from `connect`.
export interface VariantGuide {
  models: string;
  steps: string[];
  connect: ConnectMethod;
  notes: string[];
  sources: string[];
}

export interface BrandGuide {
  id: string;
  name: string;
  variants: VariantGuide[];
}

export const SOURCE_NAME = setup.source.name;
export const SOURCE_URL = `https://github.com/${setup.source.repo}/tree/${setup.source.commit}`;

export function sourceFileUrl(path: string): string {
  return `https://github.com/${setup.source.repo}/blob/${setup.source.commit}/${path}`;
}

export function brandList(): { id: string; name: string }[] {
  return setup.brands.map(({ id, name }) => ({ id, name }));
}

const menu = (path: string[]) => path.join(SEPARATOR);

function genericUnlock(): string {
  const [first, ...others] = setup.generic.unlock;
  const rest = others.map((u) => ` ${u.when ?? "Some TVs"}: ${menu(u.path)}, then ${u.action}.`).join("");
  return `If Developer options isn't there yet, open ${menu(first.path)} and ${first.action}.${rest}`;
}

function toggleText(t: Toggle): string {
  return `${t.name}${t.note ? ` (${t.note})` : ""}`;
}

function turnOn(toggles: Toggle[]): string {
  const names = toggles.map(toggleText);
  const list = names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  const then = toggles.find((t) => t.then)?.then;
  return `Turn on ${list}${then ? `, then choose ${then}` : ""}.`;
}

function variantGuide(v: Variant): VariantGuide {
  const steps: string[] = [];
  if (v.unlock) steps.push(`Show Developer options: open ${menu(v.unlock.path)} and ${v.unlock.action}.`);
  else if (v.generic_unlock !== false) steps.push(genericUnlock());
  steps.push(v.developer_options ? `Open ${menu(v.developer_options)}.` : setup.generic.developer_options);
  steps.push(turnOn(v.turn_on));
  return {
    models: v.models,
    steps,
    connect: v.connect,
    notes: [...v.notes],
    sources: v.sources.map(sourceFileUrl),
  };
}

/// The general steps, for a brand the data doesn't cover or "not sure".
export function genericGuide(): VariantGuide {
  return {
    models: "Most Android TV and Google TV models",
    steps: [genericUnlock(), setup.generic.developer_options, setup.generic.turn_on],
    connect: "either",
    notes: [],
    sources: setup.generic.sources.map(sourceFileUrl),
  };
}

export function brandGuide(id: string | null | undefined): BrandGuide | null {
  const brand = setup.brands.find((b) => b.id === id);
  return brand ? { id: brand.id, name: brand.name, variants: brand.variants.map(variantGuide) } : null;
}

const STORAGE_KEY = "atvopt.connectionBrand";

/// The brand last picked in this app. A convenience only: it isn't about any
/// one TV, so it isn't filed under a hardware id.
export function savedBrand(storage: Storage): string | null {
  try {
    const id = storage.getItem(STORAGE_KEY);
    return id && setup.brands.some((b) => b.id === id) ? id : null;
  } catch {
    return null;
  }
}

export function saveBrand(storage: Storage, id: string | null): void {
  try {
    if (id) storage.setItem(STORAGE_KEY, id);
    else storage.removeItem(STORAGE_KEY);
  } catch {
    /* storage unavailable: the pick lasts for this screen */
  }
}
