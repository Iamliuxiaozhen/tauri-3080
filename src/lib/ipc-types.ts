/**
 * IPC layer.
 *
 * Every backend interaction goes through this module so the rest of the UI can
 * be unit-tested with a fake implementation. The functions mirror the Rust
 * commands one-to-one (see `src-tauri/src/commands.rs`).
 */

export interface AppSettings {
  locale: string;
  display_name: string;
  icon_path: string | null;
  default_profile: string;
  close_to_tray: boolean;
  window: { width: number; height: number };
}

export type PrelaunchMode = 'run' | 'wait-for-port' | 'skip';

export interface Profile {
  id: string;
  name: string;
  url: string;
  icon_path: string | null;
  prelaunch: PrelaunchMode;
  script: string | null;
  args: string[];
  env: Record<string, string>;
  timeout_secs: number;
  wait_timeout_secs: number;
  kill_previous: boolean;
}

export interface PlatformInfo {
  os: string;
  arch: string;
  script_name: string;
  shell: string;
}

export interface Paths {
  config_dir: string;
  config_file: string;
  scripts_dir: string;
  icons_dir: string;
  logs_dir: string;
}

export type WindowKind = 'main' | 'settings';

export interface Bootstrap {
  settings: AppSettings;
  profiles: Profile[];
  platform: PlatformInfo;
  paths: Paths;
  version: string;
  origin: { label: string; kind: WindowKind; profile_id: string | null };
  repairs: string[];
}

export interface ProfileInput {
  name: string;
  url: string;
  iconPath?: string | null;
  prelaunch: PrelaunchMode;
  script?: string | null;
  args?: string[];
  env?: Record<string, string>;
  timeoutSecs?: number;
  waitTimeoutSecs?: number;
  killPrevious?: boolean;
}

export interface SettingsInput {
  locale: string;
  displayName: string;
  defaultProfile?: string;
  closeToTray?: boolean;
}

export type PrelaunchPhase = 'resolving' | 'starting' | 'running' | 'waiting-for-port' | 'finished';

export interface PrelaunchProgress {
  phase: PrelaunchPhase;
  elapsed_ms: number;
  lines: string[];
}

export type OutcomeStatus =
  'skipped' | 'succeeded' | 'failed' | 'timed-out' | 'error' | 'cancelled';

export interface PrelaunchOutcome {
  profile_id: string;
  script: string | null;
  status: OutcomeStatus;
  exit_code: number | null;
  duration_ms: number;
  log_path: string | null;
  log_tail: string[];
  port_ready: boolean | null;
  message: string | null;
}

export interface PrelaunchRunResult {
  outcome: PrelaunchOutcome;
  cancelled: boolean;
}

export interface IconUpdate {
  iconPath: string | null;
  custom: boolean;
}

export interface ConfigChangedEvent {
  settings: AppSettings;
  profiles: Profile[];
  repairs: string[];
}

/** The IPC surface the UI depends on. */
export interface Bridge {
  bootstrap(): Promise<Bootstrap>;
  getConfig(): Promise<{ settings: AppSettings; profile: Record<string, Profile> }>;
  reloadConfig(): Promise<string[]>;
  saveSettings(input: SettingsInput): Promise<string[]>;
  createProfile(input: ProfileInput): Promise<Profile>;
  updateProfile(id: string, input: ProfileInput): Promise<Profile>;
  deleteProfile(id: string): Promise<string[]>;
  pickAppIcon(): Promise<IconUpdate | null>;
  setAppIcon(path: string): Promise<IconUpdate>;
  resetAppIcon(): Promise<IconUpdate>;
  setProfileIcon(id: string, path: string): Promise<IconUpdate>;
  runPrelaunch(id: string): Promise<PrelaunchRunResult>;
  cancelPrelaunch(id: string): Promise<boolean>;
  openProfileUrl(url: string): Promise<void>;
  goHome(): Promise<void>;
  openWindow(profileId?: string): Promise<void>;
  openSettings(): Promise<void>;
  closeWindow(): Promise<void>;
  revealPath(path: string): Promise<void>;
  ensureScriptDir(id: string): Promise<string>;
  windowKind(): Promise<WindowKind>;
  reportReady(): Promise<void>;
  reportFrontendError(message: string): Promise<void>;
  configAsToml(): Promise<string>;
  appIconDataUrl(): Promise<string>;
  onPrelaunchProgress(handler: (progress: PrelaunchProgress) => void): Promise<() => void>;
  onConfigChanged(handler: (payload: ConfigChangedEvent) => void): Promise<() => void>;
}

/** Shape injected by `src-tauri/src/ui_script.rs` before the page runs. */
export interface WindowBootstrapGlobal {
  label: string;
  kind: WindowKind;
  profileId: string | null;
}

declare global {
  interface Window {
    __TAURI3080__?: WindowBootstrapGlobal;
    __TAURI_INTERNALS__?: unknown;
    __TAURI3080_READY__?: boolean;
    __TAURI3080_SWITCH_PROFILE__?: (profileId: string) => void;
  }
}

/** True when the page runs inside the Tauri webview. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && typeof window.__TAURI_INTERNALS__ !== 'undefined';
}

/** Reads the per-window bootstrap object. */
export function windowBootstrap(): WindowBootstrapGlobal {
  return (
    window.__TAURI3080__ ?? {
      label: 'main',
      kind: 'main',
      profileId: null,
    }
  );
}
