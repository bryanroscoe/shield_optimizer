export type SourceForm = "active_disable" | "commented_out" | "comment_reference";

export interface SourceEntry {
  package: string;
  line: number;
  upstream_form: SourceForm;
}

export interface SourceProfile {
  profile: string;
  source: string;
  metadata_source: string;
  upstream_context: { tv: string; os: string; brand: string; notes: string; readme_os?: string };
  verified_model: null;
  verified_firmware: null;
  verified_platform: null;
  entries: SourceEntry[];
}

export interface ProfileCatalog {
  upstream_commit: string;
  review_status: "unreviewed";
  actionable: false;
  profiles: SourceProfile[];
}

export function matchingProfiles(profiles: SourceProfile[], deviceQuery: string, packageQuery: string): SourceProfile[] {
  const device = deviceQuery.trim().toLowerCase();
  const packageId = packageQuery.trim().toLowerCase();
  return profiles.filter((profile) => {
    const context = profile.upstream_context;
    const deviceMatch = `${profile.profile} ${context.tv} ${context.os} ${context.brand}`.toLowerCase().includes(device);
    return deviceMatch && (!packageId || profile.entries.some((entry) => entry.package.toLowerCase().includes(packageId)));
  });
}

export function packageObservations(profile: SourceProfile, query: string): { package: string; entries: SourceEntry[] }[] {
  const needle = query.trim().toLowerCase();
  const packages = new Map<string, SourceEntry[]>();
  for (const entry of profile.entries) {
    if (!entry.package.toLowerCase().includes(needle)) continue;
    const existing = packages.get(entry.package) ?? [];
    existing.push(entry);
    packages.set(entry.package, existing);
  }
  return [...packages].sort(([a], [b]) => a.localeCompare(b)).map(([packageId, entries]) => ({ package: packageId, entries }));
}

export function sourceFormLabel(form: SourceForm): string {
  return {
    active_disable: "Upstream disable entry",
    commented_out: "Commented out upstream",
    comment_reference: "Source note",
  }[form];
}

export function sourceUrl(commit: string, profile: SourceProfile, line?: number): string {
  return `https://github.com/Gathin23/cleanyourtv/blob/${commit}/${profile.source}${line ? `#L${line}` : ""}`;
}
