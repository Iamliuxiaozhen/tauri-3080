/**
 * A fake bridge for developing the UI in a plain browser.
 *
 * `npm run dev` serves the pages without a Tauri runtime, so every command
 * would fail. Adding `?mock` to the URL swaps in this bridge, which returns
 * representative data and simulates a hook run. It is compiled into the dev
 * build only (`import.meta.env.DEV`) and never shipped.
 */

import type {
  AppSettings,
  Bootstrap,
  Bridge,
  ConfigChangedEvent,
  IconUpdate,
  PrelaunchOutcome,
  PrelaunchProgress,
  PrelaunchRunResult,
  Profile,
  ProfileInput,
  SettingsInput,
  WindowKind,
} from '../lib/ipc-types';

const SETTINGS: AppSettings = {
  locale: 'en',
  display_name: '127.0.0.1:3080',
  icon_path: null,
  default_profile: 'default',
  close_to_tray: false,
  window: { width: 1280, height: 860 },
};

const PROFILES: Profile[] = [
  {
    id: 'default',
    name: '127.0.0.1:3080',
    url: 'http://127.0.0.1:3080',
    icon_path: null,
    prelaunch: 'run',
    script: null,
    args: [],
    env: {},
    timeout_secs: 30,
    wait_timeout_secs: 60,
    kill_previous: true,
  },
  {
    id: 'docs',
    name: 'Docs preview',
    url: 'http://127.0.0.1:4000',
    icon_path: null,
    prelaunch: 'wait-for-port',
    script: 'pre-launch.sh',
    args: ['--fast'],
    env: { NODE_ENV: 'development' },
    timeout_secs: 15,
    wait_timeout_secs: 45,
    kill_previous: true,
  },
  {
    id: 'scratch',
    name: 'Scratch',
    url: 'https://example.com',
    icon_path: null,
    prelaunch: 'skip',
    script: null,
    args: [],
    env: {},
    timeout_secs: 30,
    wait_timeout_secs: 60,
    kill_previous: false,
  },
];

const PATHS = {
  config_dir: '/home/you/.config/tauri-3080',
  config_file: '/home/you/.config/tauri-3080/config.toml',
  scripts_dir: '/home/you/.config/tauri-3080/scripts',
  icons_dir: '/home/you/.config/tauri-3080/icons',
  logs_dir: '/home/you/.config/tauri-3080/logs',
};

/** Locale query parameter (`?mock&locale=zh-CN`) for quick visual checks. */
function mockLocale(): string {
  const requested = new URLSearchParams(window.location.search).get('locale');
  return requested ?? SETTINGS.locale;
}

/** Builds the fake bridge. */
export function createMockBridge(kind: WindowKind = 'main'): Bridge {
  let settings: AppSettings = { ...SETTINGS, locale: mockLocale() };
  let profiles: Profile[] = PROFILES.map((profile) => ({ ...profile }));
  const listeners = new Set<(payload: ConfigChangedEvent) => void>();
  const progressListeners = new Set<(progress: PrelaunchProgress) => void>();

  const emitChanged = (): void => {
    const payload = {
      settings: { ...settings },
      profiles: profiles.map((profile) => ({ ...profile })),
      repairs: [],
    };
    for (const listener of listeners) listener(payload);
  };

  const bootstrap = async (): Promise<Bootstrap> => ({
    settings: { ...settings },
    profiles: profiles.map((profile) => ({ ...profile })),
    platform: { os: 'linux', arch: 'x86_64', script_name: 'pre-launch.sh', shell: 'sh' },
    paths: PATHS,
    version: '0.1.0',
    origin: {
      label: kind === 'main' ? 'main' : 'settings',
      kind,
      profile_id: kind === 'main' ? 'default' : null,
    },
    repairs: [],
  });

  const outcome = (
    status: PrelaunchOutcome['status'],
    message: string | null,
  ): PrelaunchOutcome => ({
    profile_id: 'default',
    script: 'scripts/default/pre-launch.sh',
    status,
    exit_code: status === 'succeeded' ? 0 : null,
    duration_ms: 1240,
    log_path: 'logs/prelaunch-default.log',
    log_tail: [
      '$ scripts/default/pre-launch.sh',
      'pre-launch: nothing to do',
      'pre-launch: hook finished',
    ],
    port_ready: status === 'timed-out' ? false : null,
    message,
  });

  return {
    bootstrap,
    getConfig: async () => ({
      settings: { ...settings },
      profile: Object.fromEntries(profiles.map((profile) => [profile.id, profile])),
    }),
    reloadConfig: async () => [],
    configAsToml: async () => '# mock configuration\n',
    appIconDataUrl: async () => '/favicon.png',
    saveSettings: async (input: SettingsInput) => {
      settings = {
        ...settings,
        locale: input.locale,
        display_name: input.displayName || settings.display_name,
        default_profile: input.defaultProfile ?? settings.default_profile,
        close_to_tray: input.closeToTray ?? settings.close_to_tray,
      };
      emitChanged();
      return [];
    },
    createProfile: async (input: ProfileInput) => {
      const profile: Profile = {
        id: `${input.name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}-${profiles.length}`,
        name: input.name,
        url: input.url,
        icon_path: null,
        prelaunch: input.prelaunch,
        script: input.script ?? null,
        args: input.args ?? [],
        env: input.env ?? {},
        timeout_secs: input.timeoutSecs ?? 30,
        wait_timeout_secs: input.waitTimeoutSecs ?? 60,
        kill_previous: input.killPrevious ?? true,
      };
      profiles = [...profiles, profile];
      emitChanged();
      return profile;
    },
    updateProfile: async (id: string, input: ProfileInput) => {
      profiles = profiles.map((profile) =>
        profile.id === id
          ? {
              ...profile,
              name: input.name,
              url: input.url,
              prelaunch: input.prelaunch,
              script: input.script ?? null,
              timeout_secs: input.timeoutSecs ?? profile.timeout_secs,
              wait_timeout_secs: input.waitTimeoutSecs ?? profile.wait_timeout_secs,
            }
          : profile,
      );
      emitChanged();
      return profiles.find((profile) => profile.id === id) as Profile;
    },
    deleteProfile: async (id: string) => {
      profiles = profiles.filter((profile) => profile.id !== id);
      emitChanged();
      return [];
    },
    pickAppIcon: async () => null,
    setAppIcon: async (): Promise<IconUpdate> => ({ iconPath: null, custom: true }),
    resetAppIcon: async (): Promise<IconUpdate> => ({ iconPath: null, custom: false }),
    setProfileIcon: async (): Promise<IconUpdate> => ({ iconPath: null, custom: true }),
    runPrelaunch: async (id: string): Promise<PrelaunchRunResult> => {
      const profile = profiles.find((candidate) => candidate.id === id) ?? profiles[0];
      if (!profile) throw new Error('no profile');
      const steps: PrelaunchProgress[] = [
        { phase: 'resolving', elapsed_ms: 60, lines: [] },
        { phase: 'starting', elapsed_ms: 120, lines: ['$ scripts/default/pre-launch.sh'] },
        { phase: 'running', elapsed_ms: 600, lines: ['pre-launch: nothing to do'] },
      ];
      for (const step of steps) {
        for (const listener of progressListeners) listener(step);
        await new Promise((resolve) => setTimeout(resolve, 400));
      }
      return { outcome: outcome('succeeded', null), cancelled: false };
    },
    cancelPrelaunch: async () => true,
    openProfileUrl: async (url: string) => {
      // The real command navigates the window; the mock shows a toast instead.
      const toast = document.getElementById('toast');
      if (toast) {
        toast.textContent = `[mock] would open ${url}`;
        toast.hidden = false;
        window.setTimeout(() => {
          toast.hidden = true;
        }, 4000);
      } else {
        console.info('[mock] would navigate to', url);
      }
    },
    goHome: async () => {},
    openWindow: async () => {},
    openSettings: async () => {
      window.open('settings.html?mock', '_blank');
    },
    closeWindow: async () => {},
    revealPath: async () => {},
    ensureScriptDir: async (id: string) => `${PATHS.scripts_dir}/${id}`,
    windowKind: async () => kind,
    reportReady: async () => {
      window.__TAURI3080_READY__ = true;
    },
    reportFrontendError: async (message: string) => {
      console.error('[mock] frontend error', message);
    },
    onPrelaunchProgress: async (handler) => {
      progressListeners.add(handler);
      return () => progressListeners.delete(handler);
    },
    onConfigChanged: async (handler) => {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  };
}

/** True when the page should use the mock bridge. */
export function mockRequested(): boolean {
  return import.meta.env.DEV && new URLSearchParams(window.location.search).has('mock');
}
