// What the root layout lends to its pages: the update flow and the dialogs it
// owns. Settings drives the same flow as the header badge rather than a copy
// of it, so there is one download, one restart prompt and one set of notes.
import { getContext, setContext } from "svelte";

export interface AppShell {
  readonly currentVersion: string | null;
  /// The version the updater would install, or "" when there is none.
  readonly pendingVersion: string;
  readonly updateBusy: boolean;
  readonly updateInstalled: boolean;
  readonly updateProgress: string;
  readonly restartFailed: boolean;
  /// A published tag the updater manifest has not caught up with yet.
  readonly rollingOutVersion: string | null;
  readonly checking: boolean;
  /// The outcome of the last check started from Settings, "" before one.
  readonly checkMessage: string;
  checkForUpdates(): Promise<void>;
  openNotes(): void;
  openHistory(): void;
  restartApp(): Promise<void>;
  openBugReport(): Promise<void>;
}

const KEY = Symbol("app-shell");

export function provideAppShell(shell: AppShell): void {
  setContext(KEY, shell);
}

export function useAppShell(): AppShell {
  return getContext<AppShell>(KEY);
}
