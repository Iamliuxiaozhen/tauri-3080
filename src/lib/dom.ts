/** Tiny DOM helpers shared by both surfaces (no framework, no shadow DOM). */

export function byId<T extends HTMLElement = HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) throw new Error(`missing element #${id}`);
  return element as T;
}

export function query<T extends Element = Element>(root: ParentNode, selector: string): T {
  const element = root.querySelector(selector);
  if (!element) throw new Error(`missing element ${selector}`);
  return element as T;
}

export function show(element: HTMLElement | null, visible = true): void {
  if (element) element.hidden = !visible;
}

export function setText(element: HTMLElement | null, value: string): void {
  if (element) element.textContent = value;
}

export function setBusy(root: HTMLElement, busy: boolean): void {
  root.setAttribute('aria-busy', String(busy));
}

/** Keeps the newest `limit` lines; used by the hook log view. */
export function tail(lines: readonly string[], limit: number): string[] {
  return lines.length <= limit ? [...lines] : lines.slice(lines.length - limit);
}

/** Turns an unknown thrown value into a message string. */
export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === 'object' && 'message' in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

/** Adds a listener and returns the disposer. */
export function on<K extends keyof HTMLElementEventMap>(
  element: HTMLElement,
  event: K,
  handler: (event: HTMLElementEventMap[K]) => void,
): () => void {
  element.addEventListener(event, handler as EventListener);
  return () => element.removeEventListener(event, handler as EventListener);
}
