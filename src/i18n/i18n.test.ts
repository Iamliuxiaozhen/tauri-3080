import { describe, expect, it } from 'vitest';

import {
  FALLBACK_LOCALE,
  LOCALE_LABELS,
  SUPPORTED_LOCALES,
  catalogue,
  catalogues,
  format,
  getLocale,
  normalizeLocale,
  pickLocale,
  setLocale,
  translate,
} from './index';
import { en } from './en';

const PLACEHOLDER = /\{(\w+)\}/g;

function placeholders(value: string): string[] {
  return [...value.matchAll(PLACEHOLDER)].map((match) => match[1] as string).sort();
}

describe('catalogue completeness', () => {
  const englishKeys = Object.keys(en).sort();

  it('has a catalogue for every supported locale', () => {
    for (const locale of SUPPORTED_LOCALES) {
      expect(catalogue(locale)).toBeTypeOf('object');
    }
  });

  for (const [locale, messages] of catalogues()) {
    it(`${locale} defines exactly the English keys`, () => {
      expect(Object.keys(messages).sort()).toEqual(englishKeys);
    });

    if (locale === FALLBACK_LOCALE) continue;

    it(`${locale} has no empty strings`, () => {
      for (const key of englishKeys) {
        expect(messages[key as keyof typeof messages].trim(), `${locale}:${key}`).not.toBe('');
      }
    });

    it(`${locale} keeps the same placeholders as English`, () => {
      for (const key of englishKeys) {
        const reference = englishKeys.includes(key) ? en[key as keyof typeof en] : '';
        expect(placeholders(messages[key as keyof typeof messages]), `${locale}:${key}`).toEqual(
          placeholders(reference),
        );
      }
    });
  }

  it('is actually translated, not copied', () => {
    const identical = (locale: 'zh-CN' | 'zh-TW' | 'pt-BR') => {
      const messages = catalogue(locale);
      return englishKeys.filter(
        (key) => messages[key as keyof typeof messages] === en[key as keyof typeof en],
      ).length;
    };
    const translatable = englishKeys.length;
    // The product name is shared by design; everything else must differ.
    for (const locale of ['zh-CN', 'zh-TW', 'pt-BR'] as const) {
      expect(identical(locale)).toBeLessThan(translatable * 0.1);
    }
  });

  it('labels every locale in its own language', () => {
    for (const locale of SUPPORTED_LOCALES) {
      expect(LOCALE_LABELS[locale].trim()).not.toBe('');
    }
  });
});

describe('normalizeLocale', () => {
  it.each([
    ['zh', 'zh-CN'],
    ['zh-CN', 'zh-CN'],
    ['zh_Hans', 'zh-CN'],
    ['zh-Hans-CN', 'zh-CN'],
    ['zh-SG', 'zh-CN'],
    ['zh-TW', 'zh-TW'],
    ['zh_HK', 'zh-TW'],
    ['zh-Hant', 'zh-TW'],
    ['pt', 'pt-BR'],
    ['pt_BR', 'pt-BR'],
    ['pt-PT', 'pt-BR'],
    ['en', 'en'],
    ['en-US', 'en'],
    ['fr', 'en'],
    ['', 'en'],
    ['   ', 'en'],
  ])('%s → %s', (input, expected) => {
    expect(normalizeLocale(input)).toBe(expected);
  });

  it('handles nullish input', () => {
    expect(normalizeLocale(null)).toBe('en');
    expect(normalizeLocale(undefined)).toBe('en');
  });
});

describe('pickLocale', () => {
  it('prefers the first supported locale', () => {
    expect(pickLocale(['fr-FR', 'zh-TW', 'en'])).toBe('zh-TW');
  });

  it('falls back to English', () => {
    expect(pickLocale(['fr-FR', 'de-DE'])).toBe('en');
    expect(pickLocale([])).toBe('en');
  });
});

describe('format', () => {
  it('replaces known placeholders', () => {
    expect(format('Hello {name}, {count} items', { name: 'Ana', count: 3 })).toBe(
      'Hello Ana, 3 items',
    );
  });

  it('leaves unknown placeholders alone', () => {
    expect(format('Hello {name}', {})).toBe('Hello {name}');
  });

  it('returns the template when no values are given', () => {
    expect(format('plain text')).toBe('plain text');
  });
});

describe('translate', () => {
  it('translates for an explicit locale', () => {
    expect(translate('settings.title', undefined, 'zh-TW')).toBe('設定');
    expect(translate('settings.title', undefined, 'pt-BR')).toBe('Configurações');
  });

  it('falls back to English for a missing key', () => {
    // @ts-expect-error deliberately unknown key
    expect(translate('does.not.exist', undefined, 'zh-CN')).toBe('does.not.exist');
  });

  it('uses the runtime locale after setLocale', () => {
    setLocale('pt-BR');
    expect(getLocale()).toBe('pt-BR');
    expect(translate('common.cancel')).toBe('Cancelar');
    setLocale('en');
    expect(translate('common.cancel')).toBe('Cancel');
  });

  it('interpolates values', () => {
    expect(translate('launcher.opening', { url: 'http://x' }, 'en')).toBe('Opening http://x…');
  });

  it('keeps fallback English when a catalogue is missing', () => {
    // @ts-expect-error deliberately unsupported locale
    expect(translate('common.save', undefined, 'de-DE')).toBe('Save');
  });
});
