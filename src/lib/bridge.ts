/**
 * The real Tauri bridge.
 *
 * `@tauri-apps/api` is imported lazily so the Vite preview build (and the unit
 * tests) can load the modules without a Tauri runtime.
 */

import type {
  Bootstrap,
  Bridge,
  ConfigChangedEvent,
  IconUpdate,
  PrelaunchProgress,
  PrelaunchRunResult,
  Profile,
  ProfileInput,
  SettingsInput,
  WindowKind,
} from './ipc-types';

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

let invokePromise: Promise<Invoke> | null = null;

function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  invokePromise ??= import('@tauri-apps/api/core').then((module) => module.invoke as Invoke);
  return invokePromise.then((call) => call<T>(command, args));
}

interface TauriEvent<T> {
  payload: T;
}

async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const module = await import('@tauri-apps/api/event');
  const unlisten = await module.listen<T>(event, (incoming: TauriEvent<T>) => {
    handler(incoming.payload);
  });
  return unlisten;
}

/** Creates the bridge backed by the real backend. */
export function createTauriBridge(): Bridge {
  return {
    bootstrap: () => invoke<Bootstrap>('bootstrap'),
    getConfig: () => invoke('get_config'),
    reloadConfig: () => invoke<string[]>('reload_config'),
    configAsToml: () => invoke<string>('config_as_toml'),
    appIconDataUrl: () => invoke<string>('app_icon_data_url'),
    saveSettings: (input: SettingsInput) => invoke<string[]>('save_settings', { input }),
    createProfile: (input: ProfileInput) => invoke<Profile>('create_profile', { input }),
    updateProfile: (id: string, input: ProfileInput) =>
      invoke<Profile>('update_profile', { id, input }),
    deleteProfile: (id: string) => invoke<string[]>('delete_profile', { id }),
    pickAppIcon: () => invoke<IconUpdate | null>('pick_app_icon'),
    setAppIcon: (path: string) => invoke<IconUpdate>('set_app_icon', { path }),
    resetAppIcon: () => invoke<IconUpdate>('reset_app_icon'),
    setProfileIcon: (id: string, path: string) =>
      invoke<IconUpdate>('set_profile_icon', { id, path }),
    runPrelaunch: (id: string) => invoke<PrelaunchRunResult>('run_prelaunch', { id }),
    cancelPrelaunch: (id: string) => invoke<boolean>('cancel_prelaunch', { id }),
    openProfileUrl: (url: string) => invoke<void>('open_profile_url', { url }),
    goHome: () => invoke<void>('go_home'),
    openWindow: (profileId?: string) =>
      invoke<void>('open_window', { profileId: profileId ?? null }),
    openSettings: () => invoke<void>('open_settings'),
    closeWindow: () => invoke<void>('close_window'),
    revealPath: (path: string) => invoke<void>('reveal_path', { path }),
    ensureScriptDir: (id: string) => invoke<string>('ensure_script_dir', { id }),
    windowKind: () => invoke<WindowKind>('window_kind'),
    reportReady: () => invoke<void>('report_ready'),
    reportFrontendError: (message: string) => invoke<void>('report_frontend_error', { message }),
    onPrelaunchProgress: (handler: (progress: PrelaunchProgress) => void) =>
      listen<PrelaunchProgress>('prelaunch-progress', handler),
    onConfigChanged: (handler: (payload: ConfigChangedEvent) => void) =>
      listen<ConfigChangedEvent>('config-changed', handler),
  };
}
