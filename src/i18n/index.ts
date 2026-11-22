/**
 * Locale registry and the tiny translation runtime.
 *
 * The catalogue is intentionally a plain object: no build-time codegen, no
 * runtime interpolation library, just enough to keep four languages honest.
 */

import { en, type Messages } from './en';
import { ptBR } from './pt-BR';
import { zhCN } from './zh-CN';
import { zhTW } from './zh-TW';

/** Locale tags supported by the first release. */
export const SUPPORTED_LOCALES = ['en', 'zh-CN', 'zh-TW', 'pt-BR'] as const;

/** Union of the supported tags. */
export type Locale = (typeof SUPPORTED_LOCALES)[number];

/** Fallback used for unknown tags. */
export const FALLBACK_LOCALE: Locale = 'en';

/** Language names for the picker, each written in its own language. */
export const LOCALE_LABELS: Record<Locale, string> = {
  en: 'English',
  'zh-CN': '简体中文',
  'zh-TW': '繁體中文',
  'pt-BR': 'Português (Brasil)',
};

const CATALOGUES: Record<Locale, Messages> = {
  en,
  'zh-CN': zhCN,
  'zh-TW': zhTW,
  'pt-BR': ptBR,
};

/** Runtime bound to the current locale; reassigned by `setLocale`. */
let current: Locale = FALLBACK_LOCALE;

/**
 * Normalises any BCP-47-ish tag to a supported locale.
 *
 * Handles the cases a user or the OS actually sends: `zh`, `zh_Hans`,
 * `zh-HK`, `pt`, `pt_PT`, `en-US`, and trailing whitespace.
 */
export function normalizeLocale(raw: string | null | undefined): Locale {
  if (!raw) return FALLBACK_LOCALE;
  const tag = raw.trim().replace(/_/g, '-').toLowerCase();
  if (tag === '') return FALLBACK_LOCALE;

  if (
    tag === 'zh' ||
    tag.startsWith('zh-cn') ||
    tag.startsWith('zh-hans') ||
    tag.startsWith('zh-sg')
  ) {
    return 'zh-CN';
  }
  if (
    tag.startsWith('zh-tw') ||
    tag.startsWith('zh-hant') ||
    tag.startsWith('zh-hk') ||
    tag.startsWith('zh-mo')
  ) {
    return 'zh-TW';
  }
  if (tag === 'pt' || tag.startsWith('pt-')) return 'pt-BR';
  if (tag === 'en' || tag.startsWith('en-')) return 'en';
  return FALLBACK_LOCALE;
}

/** Picks the best supported locale from an ordered preference list. */
export function pickLocale(preferred: readonly string[]): Locale {
  for (const candidate of preferred) {
    const normalised = normalizeLocale(candidate);
    if (CATALOGUES[normalised] && normalised !== FALLBACK_LOCALE) return normalised;
  }
  return FALLBACK_LOCALE;
}

/** The locale currently in use. */
export function getLocale(): Locale {
  return current;
}

/** Switches the runtime locale and updates `<html lang>`. */
export function setLocale(locale: Locale): void {
  current = locale;
  if (typeof document !== 'undefined') {
    document.documentElement.lang = locale;
  }
}

/** Replaces `{name}` placeholders. Unknown placeholders are left untouched. */
export function format(template: string, values?: Record<string, string | number>): string {
  if (!values) return template;
  return template.replace(/\{(\w+)\}/g, (match, key: string) =>
    Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : match,
  );
}

/**
 * Translates `key` for `locale` (defaults to the active locale).
 *
 * Falls back to English, then to the key itself, so a missing translation is
 * visible in the UI instead of rendering as `undefined`.
 */
export function translate(
  key: keyof Messages,
  values?: Record<string, string | number>,
  locale: Locale = current,
): string {
  const catalogue = CATALOGUES[locale] ?? CATALOGUES[FALLBACK_LOCALE];
  const template = catalogue[key] ?? CATALOGUES[FALLBACK_LOCALE][key] ?? String(key);
  return format(template, values);
}

/** Curried translator, handy for passing `t` around. */
export function translator(locale: Locale = current) {
  return (key: keyof Messages, values?: Record<string, string | number>) =>
    translate(key, values, locale);
}

/** The catalogue for `locale`, used by the completeness test. */
export function catalogue(locale: Locale): Messages {
  return CATALOGUES[locale];
}

/** Every locale with its catalogue, for tests and tooling. */
export function catalogues(): Array<[Locale, Messages]> {
  return SUPPORTED_LOCALES.map((locale) => [locale, CATALOGUES[locale]]);
}

export type { Messages };
