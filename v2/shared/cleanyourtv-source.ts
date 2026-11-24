// Loads the bundled CleanYourTV catalog and the lists' original text on
// demand, so neither app ships the 2.7 MB index and 427 source files in the
// screens that start up first.

import type { ProfileCatalog, SourceProfile } from "./profile-catalog";

type Data = typeof import("./cleanyourtv-data");
let dataPromise: Promise<Data> | null = null;

function loadData(): Promise<Data> {
  dataPromise ??= import("./cleanyourtv-data").catch((error) => {
    dataPromise = null;
    throw error;
  });
  return dataPromise;
}

export async function loadCleanYourTvCatalog(): Promise<ProfileCatalog> {
  return (await loadData()).profileCatalog;
}

/// The list's debloat.txt exactly as imported. Rejects when it isn't bundled.
export async function loadSourceText(profile: SourceProfile): Promise<string> {
  const text = (await loadData()).sourceTexts[`../catalogs/cleanyourtv/upstream/${profile.source}`];
  if (typeof text !== "string") throw new Error(`The source for ${profile.profile} isn't bundled.`);
  return text;
}
