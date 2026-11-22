import { describe, expect, it } from 'vitest';

import { readSeconds, validateProfileForm } from './main';

function form(overrides: Partial<Record<'name' | 'url' | 'timeout' | 'wait', string>> = {}) {
  return {
    name: 'Work',
    url: 'http://127.0.0.1:3080',
    timeout: '30',
    wait: '60',
    ...overrides,
  };
}

describe('validateProfileForm', () => {
  it('accepts a well-formed launcher', () => {
    expect(validateProfileForm(form())).toBeNull();
  });

  it('accepts https and file URLs', () => {
    expect(validateProfileForm(form({ url: 'https://example.com/chat' }))).toBeNull();
    expect(validateProfileForm(form({ url: 'file:///tmp/index.html' }))).toBeNull();
  });

  it('requires a name', () => {
    expect(validateProfileForm(form({ name: '   ' }))).toBe('settings.profiles.validation.name');
  });

  it.each(['ftp://example.com', 'javascript:alert(1)', 'http://', 'example.com', ''])(
    'rejects %s',
    (url) => {
      expect(validateProfileForm(form({ url }))).toBe('settings.profiles.validation.url');
    },
  );

  it.each([
    { timeout: '0' },
    { timeout: '601' },
    { timeout: '1.5' },
    { timeout: 'abc' },
    { wait: '0' },
    { wait: '601' },
  ])('rejects out-of-range seconds %o', (overrides) => {
    expect(validateProfileForm(form(overrides))).toBe('settings.profiles.validation.number');
  });

  it('accepts the boundaries', () => {
    expect(validateProfileForm(form({ timeout: '1', wait: '600' }))).toBeNull();
  });
});

describe('readSeconds', () => {
  function input(value: string): HTMLInputElement {
    const element = document.createElement('input');
    element.value = value;
    return element;
  }

  it('reads a valid number', () => {
    expect(readSeconds(input('45'), 30)).toBe(45);
  });

  it('falls back for invalid input', () => {
    expect(readSeconds(input(''), 30)).toBe(30);
    expect(readSeconds(input('0'), 30)).toBe(30);
    expect(readSeconds(input('900'), 30)).toBe(30);
    expect(readSeconds(input('nope'), 30)).toBe(30);
  });
});
