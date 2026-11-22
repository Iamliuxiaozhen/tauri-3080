/**
 * The launcher state machine.
 *
 * Kept free of DOM and Tauri APIs so the interesting rules — which outcome
 * navigates, when the window waits for a port, when the user may skip — are
 * unit-tested rather than clicked through.
 */

import type { PrelaunchOutcome, PrelaunchPhase, PrelaunchProgress, Profile } from './ipc-types';

export type LaunchState =
  | { kind: 'idle' }
  | { kind: 'running'; phase: PrelaunchPhase; elapsedMs: number; lines: string[] }
  | { kind: 'finished'; outcome: PrelaunchOutcome }
  | { kind: 'error'; message: string; detail?: string };

/** What the UI should do after the hook finished. */
export type LaunchDecision =
  | { action: 'navigate'; url: string; outcome: PrelaunchOutcome | null }
  | { action: 'stay'; reason: string; outcome: PrelaunchOutcome | null };

/** Maximum number of log lines kept in memory for the splash screen. */
export const MAX_LOG_LINES = 400;

/** Appends streamed lines, keeping the tail bounded. */
export function appendLines(existing: readonly string[], incoming: readonly string[]): string[] {
  if (incoming.length === 0) return [...existing];
  const merged = [...existing, ...incoming];
  return merged.length > MAX_LOG_LINES ? merged.slice(merged.length - MAX_LOG_LINES) : merged;
}

/**
 * Decides whether the window may open the profile URL.
 *
 * Rules, in order:
 * 1. `skip` and `succeeded` always open the page.
 * 2. A cancelled or failed hook keeps the splash screen, because the user may
 *    want to read the output — but "open anyway" is offered by the UI.
 * 3. `timed-out` with `port_ready === true` still opens: the hook overran its
 *    own timeout but the service is up.
 */
export function decide(outcome: PrelaunchOutcome, profile: Profile): LaunchDecision {
  const navigate = (): LaunchDecision => ({
    action: 'navigate',
    url: profile.url,
    outcome,
  });

  switch (outcome.status) {
    case 'skipped':
    case 'succeeded':
      return navigate();
    case 'timed-out':
      return outcome.port_ready === true
        ? navigate()
        : { action: 'stay', reason: 'timeout', outcome };
    case 'cancelled':
      return { action: 'stay', reason: 'cancelled', outcome };
    case 'failed':
      return { action: 'stay', reason: 'failed', outcome };
    case 'error':
    default:
      return { action: 'stay', reason: 'error', outcome };
  }
}

/** Human-readable duration, e.g. `1.2 s`. */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return '0 ms';
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes}m ${seconds}s`;
}

/**
 * Maps a profile's pre-launch mode to what the splash screen should say.
 *
 * `run` profiles show a short "running" state; `wait-for-port` profiles explain
 * that the window is waiting for the port.
 */
export function initialPhase(profile: Profile): PrelaunchPhase {
  return profile.prelaunch === 'skip' ? 'finished' : 'resolving';
}

/** True when the profile needs a pre-launch run at all. */
export function shouldRunHook(profile: Profile): boolean {
  return profile.prelaunch !== 'skip';
}

/**
 * Merges a progress event into the running state.
 *
 * Progress events arrive at a bounded rate; the reducer keeps the latest phase
 * and the accumulated log tail.
 */
export function reduceProgress(
  state: Extract<LaunchState, { kind: 'running' }>,
  progress: PrelaunchProgress,
): Extract<LaunchState, { kind: 'running' }> {
  return {
    kind: 'running',
    phase: progress.phase,
    elapsedMs: progress.elapsed_ms,
    lines: appendLines(state.lines, progress.lines),
  };
}

/** Statuses that let the user retry the hook. */
export function isRetryable(outcome: PrelaunchOutcome | null): boolean {
  if (!outcome) return true;
  return outcome.status !== 'cancelled';
}
