/**
 * The launcher window.
 *
 * Sequence:
 * 1. Ask the backend for the configuration and the profile this window is bound
 *    to (`bootstrap`).
 * 2. Run the pre-launch hook, streaming its output to the splash screen.
 * 3. On success — or when the profile says to skip — navigate the window to the
 *    profile URL through `open_profile_url`.
 *
 * The window stays on the bundled page while the hook runs. That is deliberate:
 * a page that navigated first and ran the hook afterwards would show a broken
 * connection screen for as long as the hook takes.
 */

import { byId, errorMessage, on, setBusy, setText, show } from '../lib/dom';
import type { Bridge, PrelaunchOutcome, PrelaunchProgress, Profile } from '../lib/ipc-types';
import {
  appendLines,
  decide,
  formatDuration,
  initialPhase,
  isRetryable,
  shouldRunHook,
} from '../lib/launch-flow';
import type { LaunchState } from '../lib/launch-flow';
import { setLocale, translate as t } from '../i18n';
import { createTauriBridge } from '../lib/bridge';

/** The DOM the launcher owns. */
export interface LauncherElements {
  shell: HTMLElement;
  topbar: HTMLElement;
  profileIcon: HTMLImageElement;
  profileName: HTMLElement;
  profileUrl: HTMLElement;
  reload: HTMLButtonElement;
  newWindow: HTMLButtonElement;
  settings: HTMLButtonElement;
  splash: HTMLElement;
  splashTitle: HTMLElement;
  splashDetail: HTMLElement;
  progress: HTMLElement;
  progressBar: HTMLElement;
  log: HTMLElement;
  cancel: HTMLButtonElement;
  skip: HTMLButtonElement;
  hint: HTMLElement;
  errorPanel: HTMLElement;
  errorTitle: HTMLElement;
  errorMessage: HTMLElement;
  errorLog: HTMLElement;
  retry: HTMLButtonElement;
  errorSettings: HTMLButtonElement;
}

/** Reads the element handles from the document. */
export function collectElements(): LauncherElements {
  return {
    shell: byId('app'),
    topbar: byId('topbar'),
    profileIcon: byId<HTMLImageElement>('profile-icon'),
    profileName: byId('profile-name'),
    profileUrl: byId('profile-url'),
    reload: byId<HTMLButtonElement>('reload-button'),
    newWindow: byId<HTMLButtonElement>('new-window-button'),
    settings: byId<HTMLButtonElement>('settings-button'),
    splash: byId('splash'),
    splashTitle: byId('splash-title'),
    splashDetail: byId('splash-detail'),
    progress: byId('progress-bar').parentElement as HTMLElement,
    progressBar: byId('progress-bar'),
    log: byId('splash-log'),
    cancel: byId<HTMLButtonElement>('cancel-button'),
    skip: byId<HTMLButtonElement>('skip-button'),
    hint: byId('splash-hint'),
    errorPanel: byId('error-panel'),
    errorTitle: byId('error-title'),
    errorMessage: byId('error-message'),
    errorLog: byId('error-log'),
    retry: byId<HTMLButtonElement>('retry-button'),
    errorSettings: byId<HTMLButtonElement>('error-settings-button'),
  };
}

/** Applies the static translations; call again after a language change. */
export function localize(ui: LauncherElements): void {
  setText(ui.reload, t('launcher.reload'));
  setText(ui.newWindow, t('launcher.newWindow'));
  setText(ui.settings, t('launcher.settings'));
  setText(ui.cancel, t('launcher.cancelHook'));
  setText(ui.skip, t('launcher.openAnyway'));
  setText(ui.errorTitle, t('launcher.errorTitle'));
  setText(ui.retry, t('common.retry'));
  setText(ui.errorSettings, t('launcher.settings'));
}

/** Human sentence describing the current launch phase. */
export function describePhase(
  state: LaunchState,
  profile: Pick<Profile, 'name' | 'url' | 'prelaunch'>,
): { title: string; detail: string } {
  if (state.kind === 'idle') {
    return {
      title: t('launcher.starting'),
      detail: t('launcher.startingDetail', { profile: profile.name }),
    };
  }
  if (state.kind === 'finished') {
    return describeOutcome(state.outcome, profile);
  }
  if (state.kind === 'error') {
    return { title: t('launcher.errorTitle'), detail: state.message };
  }

  switch (state.phase) {
    case 'waiting-for-port':
      return {
        title: t('launcher.waitingForPort', { url: profile.url }),
        detail: t('profile.prelaunch.waitForPortHint'),
      };
    case 'running':
    case 'starting':
      return {
        title: t('launcher.runningHook'),
        detail: t('launcher.startingDetail', { profile: profile.name }),
      };
    case 'finished':
      return {
        title: t('launcher.starting'),
        detail: t('launcher.opening', { url: profile.url }),
      };
    case 'resolving':
    default:
      return { title: t('launcher.starting'), detail: t('launcher.resolving') };
  }
}

/** Human sentence describing a finished hook. */
export function describeOutcome(
  outcome: PrelaunchOutcome,
  profile: Pick<Profile, 'url'>,
): { title: string; detail: string } {
  const duration = formatDuration(outcome.duration_ms);
  switch (outcome.status) {
    case 'skipped':
      return { title: t('launcher.skipped'), detail: outcome.message ?? '' };
    case 'succeeded':
      return {
        title: t('launcher.opening', { url: profile.url }),
        detail: t('launcher.hookFinished', { duration }),
      };
    case 'failed':
      return {
        title: t('launcher.hookFailed', { code: outcome.exit_code ?? '?' }),
        detail: outcome.message ?? t('launcher.errorHint'),
      };
    case 'timed-out':
      return {
        title: t('launcher.hookTimedOut', { seconds: Math.round(outcome.duration_ms / 1000) }),
        detail: t('launcher.portTimeout', { url: profile.url }),
      };
    case 'cancelled':
      return { title: t('launcher.hookCancelled'), detail: outcome.message ?? '' };
    case 'error':
    default:
      return {
        title: t('launcher.hookError'),
        detail: outcome.message ?? t('launcher.errorHint'),
      };
  }
}

function renderLog(ui: LauncherElements, lines: readonly string[]): void {
  if (lines.length === 0) {
    show(ui.log, false);
    return;
  }
  show(ui.log, true);
  ui.log.textContent = lines.join('\n');
  ui.log.scrollTop = ui.log.scrollHeight;
}

/** Paints `state` into the launcher window. */
export function renderState(
  ui: LauncherElements,
  state: LaunchState,
  profile: Pick<Profile, 'name' | 'url' | 'prelaunch'>,
): void {
  if (state.kind === 'error') {
    show(ui.splash, false);
    show(ui.errorPanel, true);
    setText(ui.errorMessage, state.message);
    if (state.detail) {
      show(ui.errorLog, true);
      ui.errorLog.textContent = state.detail;
    }
    setBusy(ui.shell, false);
    return;
  }

  show(ui.splash, true);
  show(ui.errorPanel, false);
  show(ui.errorLog, false);

  const { title, detail } = describePhase(state, profile);
  setText(ui.splashTitle, title);
  setText(ui.splashDetail, detail);

  if (state.kind === 'running') {
    renderLog(ui, state.lines);
    ui.progress.dataset.phase = state.phase;
    ui.progress.classList.toggle('progress--done', state.phase === 'finished');
    show(ui.cancel, state.phase !== 'finished');
    show(ui.skip, false);
    show(ui.hint, false);
  } else if (state.kind === 'finished') {
    renderLog(ui, state.outcome.log_tail);
    ui.progress.dataset.phase = 'finished';
    ui.progress.classList.add('progress--done');
    show(ui.cancel, false);
    const offline =
      state.outcome.status !== 'succeeded' &&
      state.outcome.status !== 'skipped' &&
      isRetryable(state.outcome);
    show(ui.skip, offline);
    show(ui.hint, offline);
    if (offline) {
      setText(
        ui.hint,
        state.outcome.log_path
          ? t('launcher.scriptPath', { path: state.outcome.log_path })
          : t('launcher.errorHint'),
      );
    }
  }

  setBusy(ui.shell, false);
}

function skippedOutcome(profile: Profile): PrelaunchOutcome {
  return {
    profile_id: profile.id,
    script: null,
    status: 'skipped',
    exit_code: null,
    duration_ms: 0,
    log_path: null,
    log_tail: [],
    port_ready: null,
    message: t('launcher.noScript'),
  };
}

/** Boots the launcher page. */
export async function bootLauncher(
  bridge: Bridge = createTauriBridge(),
  ui: LauncherElements = collectElements(),
): Promise<void> {
  let profile: Profile | null = null;
  const progressListener: { dispose: (() => void) | null } = { dispose: null };
  let running = false;

  const fallbackProfile: Profile = {
    id: 'unknown',
    name: '127.0.0.1:3080',
    url: '',
    icon_path: null,
    prelaunch: 'run',
    script: null,
    args: [],
    env: {},
    timeout_secs: 30,
    wait_timeout_secs: 60,
    kill_previous: true,
  };

  const runHook = async (): Promise<void> => {
    if (!profile || running) return;
    running = true;
    show(ui.splash, true);
    show(ui.errorPanel, false);

    let state: LaunchState = {
      kind: 'running',
      phase: initialPhase(profile),
      elapsedMs: 0,
      lines: [],
    };
    renderState(ui, state, profile);

    progressListener.dispose?.();
    progressListener.dispose = await bridge.onPrelaunchProgress((progress: PrelaunchProgress) => {
      if (state.kind !== 'running') return;
      state = {
        kind: 'running',
        phase: progress.phase,
        elapsedMs: progress.elapsed_ms,
        lines: appendLines(state.lines, progress.lines),
      };
      renderState(ui, state, profile as Profile);
    });

    if (!shouldRunHook(profile)) {
      const outcome = skippedOutcome(profile);
      renderState(ui, { kind: 'finished', outcome }, profile);
      running = false;
      try {
        await bridge.openProfileUrl(profile.url);
      } catch (error) {
        renderState(ui, { kind: 'error', message: errorMessage(error) }, profile);
      }
      return;
    }

    try {
      const { outcome } = await bridge.runPrelaunch(profile.id);
      progressListener.dispose?.();
      progressListener.dispose = null;
      state = { kind: 'finished', outcome };
      renderState(ui, state, profile);

      const decision = decide(outcome, profile);
      if (decision.action === 'navigate') {
        await bridge.openProfileUrl(decision.url);
      } else {
        show(ui.skip, isRetryable(outcome));
        show(ui.hint, true);
        setText(
          ui.hint,
          outcome.log_path
            ? t('launcher.hintEditScript', { path: outcome.log_path })
            : t('launcher.errorHint'),
        );
      }
    } catch (error) {
      progressListener.dispose?.();
      progressListener.dispose = null;
      renderState(ui, { kind: 'error', message: errorMessage(error) }, profile);
    } finally {
      running = false;
    }
  };

  const switchProfile = async (profileId: string): Promise<void> => {
    if (!profileId) return;
    try {
      const data = await bridge.bootstrap();
      const next = data.profiles.find((candidate) => candidate.id === profileId);
      if (!next) return;
      profile = next;
      setText(ui.profileName, next.name);
      setText(ui.profileUrl, next.url);
      document.title = `${next.name} — ${data.settings.display_name}`;
      setText(ui.hint, t('launcher.startingDetail', { profile: next.name }));
      await runHook();
    } catch (error) {
      renderState(ui, { kind: 'error', message: errorMessage(error) }, profile ?? fallbackProfile);
    }
  };

  window.__TAURI3080_SWITCH_PROFILE__ = (profileId: string) => {
    void switchProfile(profileId);
  };

  try {
    const data = await bridge.bootstrap();
    setLocale(data.settings.locale as never);
    localize(ui);

    const requested = new URLSearchParams(window.location.search).get('profile');
    const profileId = requested ?? data.origin.profile_id ?? data.settings.default_profile;
    profile =
      data.profiles.find((candidate) => candidate.id === profileId) ?? data.profiles[0] ?? null;

    if (!profile) {
      throw new Error('no launcher profile is configured');
    }

    setText(ui.profileName, profile.name);
    setText(ui.profileUrl, profile.url);
    ui.profileIcon.alt = profile.name;
    document.title = `${profile.name} — ${data.settings.display_name}`;
    show(ui.topbar, true);
    show(ui.reload, true);
    show(ui.newWindow, true);
    show(ui.settings, true);

    if (data.repairs.length > 0) {
      show(ui.hint, true);
      setText(ui.hint, data.repairs.join(' · '));
    }

    on(ui.reload, 'click', () => {
      void runHook();
    });
    on(ui.newWindow, 'click', () => {
      void bridge.openWindow(profile?.id);
    });
    on(ui.settings, 'click', () => {
      void bridge.openSettings();
    });
    on(ui.errorSettings, 'click', () => {
      void bridge.openSettings();
    });
    on(ui.retry, 'click', () => {
      void runHook();
    });
    on(ui.cancel, 'click', () => {
      if (profile) void bridge.cancelPrelaunch(profile.id);
    });
    on(ui.skip, 'click', () => {
      if (profile) void bridge.openProfileUrl(profile.url);
    });

    await bridge.reportReady();
    window.__TAURI3080_READY__ = true;
    await runHook();
  } catch (error) {
    const message = errorMessage(error);
    void bridge.reportFrontendError(
      `launcher: ${message}\n${String((error as Error)?.stack ?? '')}`,
    );
    renderState(ui, { kind: 'error', message }, profile ?? fallbackProfile);
  } finally {
    progressListener.dispose?.();
  }
}

if (typeof window !== 'undefined' && document.getElementById('splash')) {
  void bootLauncher();
}
