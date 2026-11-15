// Controller-first navigation: D-pad / left stick / arrow keys move focus spatially, A (or
// Enter) activates, B (or Esc) goes back, LB/RB switch sections, Start jumps to "Gioca".
// Gamepad input is translated into the same keyboard events so every control behaves the same.

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

type Dir = 'up' | 'down' | 'left' | 'right';

function visible(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  if (r.width === 0 || r.height === 0) return false;
  const style = getComputedStyle(el);
  return style.visibility !== 'hidden' && style.display !== 'none';
}

function scopeRoot(): ParentNode {
  const modals = document.querySelectorAll<HTMLElement>('[data-nav-scope]');
  return modals.length ? modals[modals.length - 1] : document;
}

function candidates(): HTMLElement[] {
  return [...scopeRoot().querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => visible(el) && !el.closest('[data-nav-skip]'),
  );
}

/** Pick the nearest focusable element in `dir`, favouring alignment over raw distance. */
export function moveFocus(dir: Dir): boolean {
  const all = candidates();
  const current = document.activeElement as HTMLElement | null;
  if (!current || current === document.body || !all.includes(current)) {
    const first = document.querySelector<HTMLElement>('[data-autofocus]') ?? all[0];
    first?.focus();
    return !!first;
  }
  const a = current.getBoundingClientRect();
  const ax = a.left + a.width / 2;
  const ay = a.top + a.height / 2;
  let best: HTMLElement | null = null;
  let bestScore = Infinity;
  for (const el of all) {
    if (el === current) continue;
    const b = el.getBoundingClientRect();
    const bx = b.left + b.width / 2;
    const by = b.top + b.height / 2;
    const dx = bx - ax;
    const dy = by - ay;
    let main: number;
    let cross: number;
    switch (dir) {
      case 'up':
        if (b.bottom > a.top + 2) continue;
        main = a.top - b.bottom;
        cross = Math.abs(dx);
        break;
      case 'down':
        if (b.top < a.bottom - 2) continue;
        main = b.top - a.bottom;
        cross = Math.abs(dx);
        break;
      case 'left':
        if (b.right > a.left + 2) continue;
        main = a.left - b.right;
        cross = Math.abs(dy);
        break;
      case 'right':
        if (b.left < a.right - 2) continue;
        main = b.left - a.right;
        cross = Math.abs(dy);
        break;
    }
    const score = Math.max(0, main) + cross * 2.2;
    if (score < bestScore) {
      bestScore = score;
      best = el;
    }
  }
  if (best) {
    best.focus();
    best.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    return true;
  }
  return false;
}

function isTextEntry(el: Element | null): boolean {
  if (!el) return false;
  if (el instanceof HTMLTextAreaElement) return true;
  if (el instanceof HTMLInputElement) {
    return ['text', 'search', 'password', 'number', 'url', 'email'].includes(el.type);
  }
  return false;
}

/** Elements that consume left/right themselves (sliders, segmented controls). */
function consumesHorizontal(el: Element | null): boolean {
  return !!el && (el.matches('input[type="range"]') || !!el.closest('[data-horizontal]'));
}

export function installKeyboardNav(onBack: () => void, onTab: (delta: number) => void) {
  window.addEventListener('keydown', (e) => {
    if (e.defaultPrevented) return;
    const el = document.activeElement;
    const map: Record<string, Dir> = { ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right' };
    const dir = map[e.key];
    if (dir) {
      if (isTextEntry(el) && (dir === 'left' || dir === 'right')) return;
      if (consumesHorizontal(el) && (dir === 'left' || dir === 'right')) return;
      if (moveFocus(dir)) e.preventDefault();
      return;
    }
    if (e.key === 'Escape') {
      onBack();
      e.preventDefault();
    } else if (e.key === 'PageUp' || (e.key === 'Tab' && e.ctrlKey && e.shiftKey)) {
      onTab(-1);
      e.preventDefault();
    } else if (e.key === 'PageDown' || (e.key === 'Tab' && e.ctrlKey)) {
      onTab(1);
      e.preventDefault();
    }
  });
}

// ------------------------------------------------------------------------------------------
// Gamepad (standard mapping): 0 A, 1 B, 4 LB, 5 RB, 9 Start, 12-15 D-pad.

export function installGamepadNav(handlers: {
  back: () => void;
  tab: (delta: number) => void;
  start: () => void;
  connected: (name: string | null) => void;
}) {
  const held = new Map<string, number>();
  let lastName: string | null = null;
  const REPEAT_DELAY = 360;
  const REPEAT_RATE = 110;

  const fireKey = (key: string) => {
    const target = (document.activeElement as HTMLElement | null) ?? document.body;
    const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
    target.dispatchEvent(event);
  };

  const press = (id: string, down: boolean, now: number, action: () => void, repeat = false) => {
    const since = held.get(id);
    if (!down) {
      held.delete(id);
      return;
    }
    if (since === undefined) {
      held.set(id, now);
      action();
    } else if (repeat && now - since > REPEAT_DELAY) {
      held.set(id, now - REPEAT_DELAY + REPEAT_RATE);
      action();
    }
  };

  const loop = (now: number) => {
    const pads = navigator.getGamepads?.() ?? [];
    const pad = [...pads].find((p) => p && p.connected) ?? null;
    const name = pad ? pad.id : null;
    if (name !== lastName) {
      lastName = name;
      handlers.connected(name);
    }
    if (pad && document.hasFocus()) {
      const b = (i: number) => !!pad.buttons[i]?.pressed;
      const ax = pad.axes[0] ?? 0;
      const ay = pad.axes[1] ?? 0;
      press('up', b(12) || ay < -0.6, now, () => fireKey('ArrowUp'), true);
      press('down', b(13) || ay > 0.6, now, () => fireKey('ArrowDown'), true);
      press('left', b(14) || ax < -0.6, now, () => fireKey('ArrowLeft'), true);
      press('right', b(15) || ax > 0.6, now, () => fireKey('ArrowRight'), true);
      press('a', b(0), now, () => {
        const el = document.activeElement as HTMLElement | null;
        if (el && el !== document.body) el.click();
        else moveFocus('down');
      });
      press('b', b(1), now, handlers.back);
      press('lb', b(4), now, () => handlers.tab(-1));
      press('rb', b(5), now, () => handlers.tab(1));
      press('start', b(9), now, handlers.start);
    }
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}
