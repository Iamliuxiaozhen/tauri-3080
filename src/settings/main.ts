/**
 * The settings window.
 *
 * One window, five sections, and a single source of truth: the backend. Every
 * save goes through an IPC command and the returned snapshot is re-rendered, so
 * the settings window can never drift from `config.toml`.
 */

import { byId, errorMessage, on, setBusy, setText, show } from '../lib/dom';
import type {
  Bootstrap,
  Bridge,
  PrelaunchMode,
  Profile,
  ProfileInput,
  SettingsInput,
} from '../lib/ipc-types';
import {
  LOCALE_LABELS,
  SUPPORTED_LOCALES,
  normalizeLocale,
  setLocale,
  translate as t,
  type Locale,
} from '../i18n';
import { createTauriBridge } from '../lib/bridge';

type Section = 'appearance' | 'profiles' | 'hooks' | 'window' | 'about';

const SECTIONS: Section[] = ['appearance', 'profiles', 'hooks', 'window', 'about'];

const SECTION_LABELS: Record<Section, Parameters<typeof t>[0]> = {
  appearance: 'settings.nav.appearance',
  profiles: 'settings.nav.profiles',
  hooks: 'settings.nav.hooks',
  window: 'settings.nav.window',
  about: 'settings.nav.about',
};

const PRELAUNCH_LABELS: Record<PrelaunchMode, Parameters<typeof t>[0]> = {
  run: 'profile.prelaunch.run',
  'wait-for-port': 'profile.prelaunch.waitForPort',
  skip: 'profile.prelaunch.skip',
};

const PRELAUNCH_HINTS: Record<PrelaunchMode, Parameters<typeof t>[0]> = {
  run: 'profile.prelaunch.runHint',
  'wait-for-port': 'profile.prelaunch.waitForPortHint',
  skip: 'profile.prelaunch.skipHint',
};

/** Settings page state. */
export interface SettingsState {
  data: Bootstrap;
  active: Section;
  editing: Profile | null;
  hooksProfile: string;
  saving: boolean;
}

function el<T extends HTMLElement = HTMLElement>(id: string): T {
  return byId<T>(id);
}

/** Builds the navigation list once. */
function renderNav(state: SettingsState, onSelect: (section: Section) => void): void {
  const list = el('nav-list');
  list.replaceChildren();
  for (const section of SECTIONS) {
    const item = document.createElement('li');
    const button = document.createElement('button');
    button.type = 'button';
    button.role = 'tab';
    button.className = 'button button--ghost';
    button.dataset.section = section;
    button.textContent = t(SECTION_LABELS[section]);
    button.setAttribute('aria-selected', String(state.active === section));
    button.addEventListener('click', () => onSelect(section));
    item.append(button);
    list.append(item);
  }
}

/** Shows the active section only. */
function renderSections(state: SettingsState): void {
  for (const section of SECTIONS) {
    const node = document.querySelector<HTMLElement>(`[data-section="${section}"]`);
    show(node, state.active === section);
  }
  for (const button of document.querySelectorAll<HTMLButtonElement>('#nav-list button')) {
    button.setAttribute('aria-selected', String(button.dataset.section === state.active));
  }
}

function renderBrand(state: SettingsState): void {
  setText(el('nav-name'), state.data.settings.display_name);
  setText(el('nav-version'), `v${state.data.version}`);
  const preview = el<HTMLImageElement>('icon-preview');
  preview.alt = state.data.settings.display_name;
  setText(el('display-name'), '');
  el<HTMLInputElement>('display-name').value = state.data.settings.display_name;
}

function renderStaticLabels(): void {
  setText(el('appearance-heading'), t('settings.appearance.heading'));
  setText(el('appearance-description'), t('settings.appearance.description'));
  setText(el('display-name-label'), t('settings.appearance.displayName'));
  setText(el('display-name-hint'), t('settings.appearance.displayNameHint'));
  setText(el('icon-label'), t('settings.appearance.icon'));
  setText(el('icon-hint'), t('settings.appearance.iconHint'));
  setText(el('pick-icon'), t('settings.appearance.pickIcon'));
  setText(el('reset-icon'), t('settings.appearance.resetIcon'));
  setText(el('locale-label'), t('settings.appearance.language'));
  setText(el('locale-hint'), t('settings.appearance.languageHint'));

  setText(el('profiles-heading'), t('settings.profiles.heading'));
  setText(el('profiles-description'), t('settings.profiles.description'));
  setText(el('add-profile'), t('settings.profiles.add'));
  setText(el('profile-form-title'), t('settings.profiles.new'));
  setText(el('profile-name-label'), t('settings.profiles.name'));
  setText(el('profile-url-label'), t('settings.profiles.url'));
  setText(el('profile-url-hint'), t('settings.profiles.urlHint'));
  setText(el('profile-mode-label'), t('settings.profiles.mode'));
  setText(el('profile-timeout-label'), t('settings.profiles.timeout'));
  setText(el('profile-wait-label'), t('settings.profiles.waitTimeout'));
  setText(el('profile-script-label'), t('settings.profiles.script'));
  setText(el('profile-save'), t('common.save'));
  setText(el('profile-cancel'), t('common.cancel'));
  setText(el('profile-delete'), t('common.delete'));

  setText(el('hooks-heading'), t('settings.hooks.heading'));
  setText(el('hooks-description'), t('settings.hooks.description'));
  setText(el('hooks-profile-label'), t('settings.hooks.profile'));
  setText(el('open-scripts'), t('settings.hooks.openFolder'));
  setText(el('hooks-hint'), t('settings.hooks.hint'));

  setText(el('window-heading'), t('settings.window.heading'));
  setText(el('window-description'), t('settings.window.description'));
  setText(el('default-profile-label'), t('settings.window.defaultProfile'));
  setText(el('close-to-tray-label'), t('settings.window.closeToTray'));
  setText(el('save-window'), t('settings.window.save'));
  setText(el('reload-config'), t('settings.window.reload'));

  setText(el('about-heading'), t('settings.about.heading'));
  setText(el('about-version-label'), t('settings.about.version'));
  setText(el('about-platform-label'), t('settings.about.platform'));
  setText(el('about-config-label'), t('settings.about.config'));
  setText(el('about-theme-label'), t('settings.about.theme'));
  setText(el('open-config'), t('settings.about.openConfig'));
  setText(el('open-logs'), t('settings.about.openLogs'));
  setText(el('about-license'), t('settings.about.license'));

  const localeSelect = el<HTMLSelectElement>('locale-select');
  localeSelect.replaceChildren();
  for (const locale of SUPPORTED_LOCALES) {
    const option = document.createElement('option');
    option.value = locale;
    option.textContent = LOCALE_LABELS[locale];
    localeSelect.append(option);
  }

  const modeSelect = el<HTMLSelectElement>('profile-mode');
  modeSelect.replaceChildren();
  for (const mode of Object.keys(PRELAUNCH_LABELS) as PrelaunchMode[]) {
    const option = document.createElement('option');
    option.value = mode;
    option.textContent = t(PRELAUNCH_LABELS[mode]);
    modeSelect.append(option);
  }

  setText(el('profile-script-hint'), t('settings.profiles.scriptHint', { name: scriptName() }));
}

/** Name of the default hook script for this platform. */
function scriptName(): string {
  return navigator.userAgent.includes('Windows') ? 'pre-launch.ps1' : 'pre-launch.sh';
}

function renderAppearance(state: SettingsState): void {
  el<HTMLInputElement>('display-name').value = state.data.settings.display_name;
  el<HTMLSelectElement>('locale-select').value = normalizeLocale(state.data.settings.locale);
  const preview = el<HTMLImageElement>('icon-preview');
  const nav = el<HTMLImageElement>('nav-icon');
  void bridge()
    .appIconDataUrl()
    .then((dataUrl) => {
      const source = dataUrl || '/favicon.png';
      preview.src = source;
      nav.src = source;
    })
    .catch(() => {
      preview.src = '/favicon.png';
      nav.src = '/favicon.png';
    });
}

function renderProfiles(state: SettingsState): void {
  const list = el('profile-list');
  list.replaceChildren();
  for (const profile of state.data.profiles) {
    const item = document.createElement('li');
    item.className = 'profile-list__item';

    const main = document.createElement('div');
    main.className = 'profile-list__main';
    const text = document.createElement('div');
    text.className = 'profile-list__text';

    const name = document.createElement('div');
    name.className = 'profile-list__name';
    name.textContent = profile.name;
    if (profile.id === state.data.settings.default_profile) {
      const badge = document.createElement('span');
      badge.className = 'badge';
      badge.textContent = t('settings.profiles.defaultBadge');
      name.append(badge);
    }

    const url = document.createElement('div');
    url.className = 'profile-list__url';
    url.textContent = profile.url;

    text.append(name, url);
    main.append(text);

    const actions = document.createElement('div');
    actions.className = 'profile-list__actions';

    const edit = document.createElement('button');
    edit.type = 'button';
    edit.className = 'button button--ghost button--small';
    edit.textContent = t('common.edit');
    edit.addEventListener('click', () => {
      state.editing = profile;
      renderProfileForm(state);
    });

    const open = document.createElement('button');
    open.type = 'button';
    open.className = 'button button--ghost button--small';
    open.textContent = t('common.open');
    open.addEventListener('click', () => {
      void bridge().openWindow(profile.id);
    });

    actions.append(open, edit);
    item.append(main, actions);
    list.append(item);
  }
}

function renderProfileForm(state: SettingsState): void {
  const form = el<HTMLFormElement>('profile-form');
  const editing = state.editing;
  show(form, Boolean(editing) || form.dataset.mode === 'new');
  if (!editing) return;

  form.dataset.mode = 'edit';
  setText(el('profile-form-title'), t('settings.profiles.edit'));
  el<HTMLInputElement>('profile-name').value = editing.name;
  el<HTMLInputElement>('profile-url').value = editing.url;
  el<HTMLSelectElement>('profile-mode').value = editing.prelaunch;
  el<HTMLInputElement>('profile-timeout').value = String(editing.timeout_secs);
  el<HTMLInputElement>('profile-wait').value = String(editing.wait_timeout_secs);
  el<HTMLInputElement>('profile-script').value = editing.script ?? '';
  show(el('profile-delete'), state.data.profiles.length > 1);
  show(el('profile-form-error'), false);
  updateModeHint();
}

function updateModeHint(): void {
  const mode = el<HTMLSelectElement>('profile-mode').value as PrelaunchMode;
  setText(el('profile-mode-hint'), t(PRELAUNCH_HINTS[mode]));
}

function renderHooks(state: SettingsState): void {
  const select = el<HTMLSelectElement>('hooks-profile');
  select.replaceChildren();
  for (const profile of state.data.profiles) {
    const option = document.createElement('option');
    option.value = profile.id;
    option.textContent = profile.name;
    select.append(option);
  }
  if (!state.data.profiles.some((profile) => profile.id === state.hooksProfile)) {
    state.hooksProfile = state.data.profiles[0]?.id ?? '';
  }
  select.value = state.hooksProfile;

  const profile = state.data.profiles.find((candidate) => candidate.id === state.hooksProfile);
  const path = profile
    ? `${state.data.paths.scripts_dir}/${profile.id}/${profile.script ?? scriptName()}`
    : '';
  setText(el('hooks-path'), path || state.data.paths.scripts_dir);
  setText(el('hooks-preview'), hooksPreview(profile));
}

function hooksPreview(profile: Profile | undefined): string {
  if (!profile) return '';
  if (profile.prelaunch === 'skip') return t('profile.prelaunch.skipHint');
  return scriptSnippet();
}

function scriptSnippet(): string {
  if (navigator.userAgent.includes('Windows')) {
    return [
      '# pre-launch.ps1 — runs before the window opens',
      'Write-Output "pre-launch: nothing to do"',
      '',
      '# Receives:',
      '#   $env:TAURI_3080_PROFILE, $env:TAURI_3080_URL, $env:TAURI_3080_CONFIG_DIR',
    ].join('\n');
  }
  return [
    '#!/usr/bin/env sh',
    '# pre-launch.sh — runs before the window opens',
    'echo "pre-launch: nothing to do"',
    '',
    '# Receives:',
    '#   $TAURI_3080_PROFILE, $TAURI_3080_URL, $TAURI_3080_CONFIG_DIR',
  ].join('\n');
}

function renderWindow(state: SettingsState): void {
  const select = el<HTMLSelectElement>('default-profile');
  select.replaceChildren();
  for (const profile of state.data.profiles) {
    const option = document.createElement('option');
    option.value = profile.id;
    option.textContent = profile.name;
    select.append(option);
  }
  select.value = state.data.settings.default_profile;
  el<HTMLInputElement>('close-to-tray').checked = state.data.settings.close_to_tray;
}

function renderAbout(state: SettingsState): void {
  setText(el('about-version'), state.data.version);
  setText(el('about-platform'), `${state.data.platform.os} · ${state.data.platform.arch}`);
  setText(el('about-config'), state.data.paths.config_file);
  const prefersLight =
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: light)').matches;
  setText(
    el('about-theme'),
    t(prefersLight ? 'settings.about.themeLight' : 'settings.about.themeDark'),
  );
}

let toastTimer: number | undefined;

function toast(message: string, kind: 'ok' | 'error' = 'ok'): void {
  const node = el('toast');
  node.textContent = message;
  node.classList.toggle('toast--error', kind === 'error');
  show(node, true);
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => show(node, false), 4000);
}

/** Renders everything from the current state. */
export function render(state: SettingsState): void {
  renderStaticLabels();
  renderSections(state);
  renderBrand(state);
  renderAppearance(state);
  renderProfiles(state);
  renderHooks(state);
  renderWindow(state);
  renderAbout(state);
}

/** Reads a positive integer input, falling back to `fallback`. */
export function readSeconds(input: HTMLInputElement, fallback: number): number {
  const value = Number.parseInt(input.value, 10);
  if (!Number.isFinite(value) || value < 1 || value > 600) return fallback;
  return value;
}

/** Validates the profile form, returning an error key or `null`. */
export function validateProfileForm(form: {
  name: string;
  url: string;
  timeout: string;
  wait: string;
}): string | null {
  if (form.name.trim().length === 0) return 'settings.profiles.validation.name';
  const url = form.url.trim().toLowerCase();
  if (!/^(https?:\/\/[^\s/?#]+|file:\/\/)/.test(url)) return 'settings.profiles.validation.url';
  const timeout = Number(form.timeout);
  const wait = Number(form.wait);
  if (!Number.isInteger(timeout) || timeout < 1 || timeout > 600) {
    return 'settings.profiles.validation.number';
  }
  if (!Number.isInteger(wait) || wait < 1 || wait > 600) {
    return 'settings.profiles.validation.number';
  }
  return null;
}

let bridgeInstance: Bridge | null = null;

function bridge(): Bridge {
  bridgeInstance ??= createTauriBridge();
  return bridgeInstance;
}

function profileInput(state: SettingsState): ProfileInput {
  return {
    name: el<HTMLInputElement>('profile-name').value.trim(),
    url: el<HTMLInputElement>('profile-url').value.trim(),
    prelaunch: el<HTMLSelectElement>('profile-mode').value as PrelaunchMode,
    script: el<HTMLInputElement>('profile-script').value.trim() || null,
    timeoutSecs: readSeconds(el<HTMLInputElement>('profile-timeout'), 30),
    waitTimeoutSecs: readSeconds(el<HTMLInputElement>('profile-wait'), 60),
    iconPath: state.editing?.icon_path ?? null,
  };
}

function showFormError(message: string | null): void {
  const node = el('profile-form-error');
  if (!message) {
    show(node, false);
    return;
  }
  node.textContent = message;
  show(node, true);
}

/** Boots the settings window. */
export async function bootSettings(api: Bridge = bridge()): Promise<void> {
  bridgeInstance = api;
  let state: SettingsState;

  const refresh = async (): Promise<void> => {
    const data = await api.bootstrap();
    state = {
      data,
      active: state?.active ?? 'appearance',
      editing: state?.editing ?? null,
      hooksProfile: state?.hooksProfile ?? data.settings.default_profile,
      saving: false,
    };
    setLocale(normalizeLocale(data.settings.locale) as Locale);
    render(state);
    renderNav(state, selectSection);
    if (data.repairs.length > 0) {
      toast(t('settings.repairs.body', { count: data.repairs.length }), 'error');
    }
  };

  const selectSection = (section: Section): void => {
    state.active = section;
    render(state);
    renderNav(state, selectSection);
  };

  const newProfile = (): void => {
    state.editing = {
      id: '',
      name: '',
      url: 'http://127.0.0.1:3080',
      icon_path: null,
      prelaunch: 'run',
      script: null,
      args: [],
      env: {},
      timeout_secs: 30,
      wait_timeout_secs: 60,
      kill_previous: true,
    };
    const form = el<HTMLFormElement>('profile-form');
    form.dataset.mode = 'new';
    show(form, true);
    setText(el('profile-form-title'), t('settings.profiles.new'));
    el<HTMLInputElement>('profile-name').value = '';
    el<HTMLInputElement>('profile-url').value = 'http://127.0.0.1:3080';
    el<HTMLSelectElement>('profile-mode').value = 'run';
    el<HTMLInputElement>('profile-timeout').value = '30';
    el<HTMLInputElement>('profile-wait').value = '60';
    el<HTMLInputElement>('profile-script').value = '';
    show(el('profile-delete'), false);
    showFormError(null);
    updateModeHint();
  };

  try {
    await refresh();
    setBusy(el('app'), false);

    // Appearance ------------------------------------------------------------
    el('display-name').addEventListener('change', () => {
      void (async () => {
        const input: SettingsInput = {
          locale: state.data.settings.locale,
          displayName: el<HTMLInputElement>('display-name').value,
        };
        try {
          await api.saveSettings(input);
          await refresh();
          toast(t('common.saved'));
        } catch (error) {
          toast(t('settings.toast.failed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    el('locale-select').addEventListener('change', () => {
      void (async () => {
        const input: SettingsInput = {
          locale: el<HTMLSelectElement>('locale-select').value,
          displayName: el<HTMLInputElement>('display-name').value,
        };
        try {
          await api.saveSettings(input);
          await refresh();
        } catch (error) {
          toast(t('settings.toast.failed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    on(el('pick-icon'), 'click', () => {
      void (async () => {
        try {
          const result = await api.pickAppIcon();
          if (!result) return;
          await refresh();
          toast(t('settings.appearance.iconUpdated'));
        } catch (error) {
          toast(t('settings.appearance.iconFailed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    on(el('reset-icon'), 'click', () => {
      void (async () => {
        try {
          await api.resetAppIcon();
          await refresh();
          toast(t('settings.appearance.iconReset'));
        } catch (error) {
          toast(t('settings.appearance.iconFailed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    // Profiles --------------------------------------------------------------
    on(el('add-profile'), 'click', () => {
      newProfile();
    });

    el<HTMLSelectElement>('profile-mode').addEventListener('change', updateModeHint);

    el<HTMLFormElement>('profile-form').addEventListener('submit', (event) => {
      event.preventDefault();
      void (async () => {
        const form = {
          name: el<HTMLInputElement>('profile-name').value,
          url: el<HTMLInputElement>('profile-url').value,
          timeout: el<HTMLInputElement>('profile-timeout').value,
          wait: el<HTMLInputElement>('profile-wait').value,
        };
        const error = validateProfileForm(form);
        if (error) {
          showFormError(t(error as never));
          return;
        }
        showFormError(null);
        try {
          if (state.editing?.id) {
            await api.updateProfile(state.editing.id, profileInput(state));
          } else {
            const created = await api.createProfile(profileInput(state));
            await api.ensureScriptDir(created.id);
          }
          state.editing = null;
          show(el('profile-form'), false);
          await refresh();
          toast(t('settings.profiles.saved'));
        } catch (err) {
          showFormError(errorMessage(err));
        }
      })();
    });

    on(el('profile-cancel'), 'click', () => {
      state.editing = null;
      show(el('profile-form'), false);
      showFormError(null);
    });

    on(el('profile-delete'), 'click', () => {
      void (async () => {
        if (!state.editing?.id) return;
        const confirmed = window.confirm(
          t('settings.profiles.confirmDelete', { name: state.editing.name }),
        );
        if (!confirmed) return;
        try {
          await api.deleteProfile(state.editing.id);
          state.editing = null;
          show(el('profile-form'), false);
          await refresh();
          toast(t('settings.profiles.deleted'));
        } catch (error) {
          toast(t('settings.toast.failed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    // Hooks -----------------------------------------------------------------
    el<HTMLSelectElement>('hooks-profile').addEventListener('change', () => {
      state.hooksProfile = el<HTMLSelectElement>('hooks-profile').value;
      renderHooks(state);
    });

    on(el('open-scripts'), 'click', () => {
      void (async () => {
        try {
          const directory =
            state.data.profiles.find((profile) => profile.id === state.hooksProfile)?.id ??
            state.hooksProfile;
          await api.ensureScriptDir(directory);
          await api.revealPath(`${state.data.paths.scripts_dir}/${directory}`);
        } catch (error) {
          toast(t('settings.toast.failed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    // Window ----------------------------------------------------------------
    on(el('save-window'), 'click', () => {
      void (async () => {
        const input: SettingsInput = {
          locale: state.data.settings.locale,
          displayName: state.data.settings.display_name,
          defaultProfile: el<HTMLSelectElement>('default-profile').value,
          closeToTray: el<HTMLInputElement>('close-to-tray').checked,
        };
        try {
          await api.saveSettings(input);
          await refresh();
          toast(t('settings.window.saved'));
        } catch (error) {
          toast(t('settings.toast.failed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    on(el('reload-config'), 'click', () => {
      void (async () => {
        try {
          const repairs = await api.reloadConfig();
          await refresh();
          toast(
            repairs.length > 0
              ? t('settings.window.reloadedWithRepairs', { count: repairs.length })
              : t('settings.window.reloaded'),
          );
        } catch (error) {
          toast(t('settings.toast.failed', { error: errorMessage(error) }), 'error');
        }
      })();
    });

    // About -----------------------------------------------------------------
    on(el('open-config'), 'click', () => {
      void api.revealPath(state.data.paths.config_dir);
    });
    on(el('open-logs'), 'click', () => {
      void api.revealPath(state.data.paths.logs_dir);
    });

    await api.onConfigChanged(() => {
      void refresh();
    });
    await api.reportReady();
    window.__TAURI3080_READY__ = true;
  } catch (error) {
    void api.reportFrontendError(`settings: ${errorMessage(error)}`);
    toast(errorMessage(error), 'error');
    setBusy(el('app'), false);
  }
}

if (typeof window !== 'undefined' && document.getElementById('nav-list')) {
  if (import.meta.env.DEV) {
    // `?mock` renders the UI without a Tauri runtime. The whole branch is
    // dropped from production builds.
    const { createMockBridge, mockRequested } = await import('../dev/mock-bridge');
    if (mockRequested()) {
      await bootSettings(createMockBridge('settings'));
    } else {
      await bootSettings();
    }
  } else {
    await bootSettings();
  }
}
