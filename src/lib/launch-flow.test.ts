import { describe, expect, it } from 'vitest';

import {
  MAX_LOG_LINES,
  appendLines,
  decide,
  formatDuration,
  initialPhase,
  isRetryable,
  reduceProgress,
  shouldRunHook,
} from './launch-flow';
import type { PrelaunchOutcome, Profile } from './ipc-types';

const profile: Profile = {
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
};

function outcome(overrides: Partial<PrelaunchOutcome> = {}): PrelaunchOutcome {
  return {
    profile_id: 'default',
    script: 'scripts/default/pre-launch.sh',
    status: 'succeeded',
    exit_code: 0,
    duration_ms: 1200,
    log_path: 'logs/prelaunch-default.log',
    log_tail: [],
    port_ready: null,
    message: null,
    ...overrides,
  };
}

describe('decide', () => {
  it('navigates after a successful hook', () => {
    const decision = decide(outcome(), profile);
    expect(decision).toMatchObject({ action: 'navigate', url: profile.url });
  });

  it('navigates when the hook was skipped', () => {
    expect(decide(outcome({ status: 'skipped' }), profile).action).toBe('navigate');
  });

  it('stays when the hook failed', () => {
    const decision = decide(outcome({ status: 'failed', exit_code: 2 }), profile);
    expect(decision.action).toBe('stay');
    expect(decision.action === 'stay' && decision.reason).toBe('failed');
  });

  it('stays when the hook was cancelled', () => {
    expect(decide(outcome({ status: 'cancelled' }), profile).action).toBe('stay');
  });

  it('stays on timeout unless the port is already serving', () => {
    expect(decide(outcome({ status: 'timed-out', port_ready: false }), profile).action).toBe(
      'stay',
    );
    expect(decide(outcome({ status: 'timed-out', port_ready: null }), profile).action).toBe('stay');
    expect(decide(outcome({ status: 'timed-out', port_ready: true }), profile).action).toBe(
      'navigate',
    );
  });

  it('stays on hard errors', () => {
    expect(decide(outcome({ status: 'error' }), profile).action).toBe('stay');
  });

  it('honours a non-default URL', () => {
    const custom = { ...profile, url: 'https://example.com/chat' };
    const decision = decide(outcome(), custom);
    expect(decision.action === 'navigate' && decision.url).toBe('https://example.com/chat');
  });
});

describe('appendLines', () => {
  it('keeps everything below the cap', () => {
    expect(appendLines(['a'], ['b', 'c'])).toEqual(['a', 'b', 'c']);
  });

  it('returns a copy for empty input', () => {
    const existing = ['a'];
    const next = appendLines(existing, []);
    expect(next).toEqual(['a']);
    expect(next).not.toBe(existing);
  });

  it('trims to the newest lines', () => {
    const existing = Array.from({ length: MAX_LOG_LINES }, (_, index) => `old-${index}`);
    const next = appendLines(existing, ['new-1', 'new-2']);
    expect(next).toHaveLength(MAX_LOG_LINES);
    expect(next.at(-1)).toBe('new-2');
    expect(next.at(-2)).toBe('new-1');
    expect(next[0]).not.toBe('old-0');
  });
});

describe('formatDuration', () => {
  it.each([
    [0, '0 ms'],
    [340, '340 ms'],
    [999, '999 ms'],
    [1000, '1.0 s'],
    [1234, '1.2 s'],
    [59_400, '59.4 s'],
    [60_000, '1m 0s'],
    [125_000, '2m 5s'],
  ])('%s ms → %s', (input, expected) => {
    expect(formatDuration(input)).toBe(expected);
  });

  it('handles invalid input', () => {
    expect(formatDuration(Number.NaN)).toBe('0 ms');
    expect(formatDuration(-5)).toBe('0 ms');
  });
});

describe('profile helpers', () => {
  it('skips hooks only for the skip mode', () => {
    expect(shouldRunHook({ ...profile, prelaunch: 'skip' })).toBe(false);
    expect(shouldRunHook({ ...profile, prelaunch: 'run' })).toBe(true);
    expect(shouldRunHook({ ...profile, prelaunch: 'wait-for-port' })).toBe(true);
  });

  it('starts in the finished phase for skip profiles', () => {
    expect(initialPhase({ ...profile, prelaunch: 'skip' })).toBe('finished');
    expect(initialPhase(profile)).toBe('resolving');
  });

  it('only offers retry for non-cancelled outcomes', () => {
    expect(isRetryable(null)).toBe(true);
    expect(isRetryable(outcome({ status: 'failed' }))).toBe(true);
    expect(isRetryable(outcome({ status: 'cancelled' }))).toBe(false);
  });
});

describe('reduceProgress', () => {
  it('accumulates lines and phase', () => {
    const first = reduceProgress(
      { kind: 'running', phase: 'resolving', elapsedMs: 0, lines: [] },
      { phase: 'running', elapsed_ms: 120, lines: ['one'] },
    );
    const second = reduceProgress(first, {
      phase: 'waiting-for-port',
      elapsed_ms: 400,
      lines: ['two'],
    });
    expect(second).toEqual({
      kind: 'running',
      phase: 'waiting-for-port',
      elapsedMs: 400,
      lines: ['one', 'two'],
    });
  });
});
