import { api, demo, on } from './api';
import { defaultSettings } from './defaults';
import { resolveLocale, setLocale, tr } from './i18n.svelte';
import type { EngineEvent, EngineKind, LaunchState, Settings, SystemInfo } from './types';

export type PageId = 'home' | 'grafica' | 'controlli' | 'audio' | 'online' | 'mod' | 'sistema';

/** Sections of the launcher; labels and hints are English source text, shown through `tr`. */
export const pages: { id: PageId; label: string; hint: string }[] = [
  { id: 'home', label: 'Play', hint: 'Start and summary' },
  { id: 'grafica', label: 'Graphics', hint: 'Resolution, upscaling, smoothness' },
  { id: 'controlli', label: 'Controls', hint: 'Controllers and shortcuts' },
  { id: 'audio', label: 'Audio', hint: 'Volume and output' },
  { id: 'online', label: 'Online', hint: 'Online races and rooms' },
  { id: 'mod', label: 'Mods', hint: 'Extra content' },
  { id: 'sistema', label: 'System', hint: 'Files, language, logs' },
];

/** The engines as players see them. */
export function engineName(engine: EngineKind): string {
  return engine === 'native' ? tr('Native') : engine === 'online' ? tr('Online') : tr('Emulator');
}

type Tone = 'info' | 'ok' | 'warn' | 'error';

class AppState {
  settings = $state<Settings>(structuredClone(defaultSettings));
  system = $state<SystemInfo | null>(null);
  page = $state<PageId>('home');
  loaded = $state(false);
  launch = $state<LaunchState>({
    running: false,
    engine: null,
    pid: null,
    startedAt: null,
    fps: null,
    frametimeMs: null,
    speed: null,
    lastError: null,
  });
  engineLog = $state<string[]>([]);
  toasts = $state<{ id: number; text: string; tone: Tone }[]>([]);
  #saveTimer: ReturnType<typeof setTimeout> | null = null;
  #toastId = 0;

  async init() {
    const [settings, system] = await Promise.all([api.getSettings(), api.systemInfo()]);
    this.settings = settings;
    this.system = system;
    this.applyLanguage();
    const start = demo?.get('page');
    if (start && pages.some((p) => p.id === start)) this.page = start as PageId;
    this.loaded = true;
    this.#listenEngine();
  }

  /** Interface language from the setting ("auto": the system's language and country). */
  applyLanguage() {
    const forced = demo?.get('lang');
    setLocale(forced === 'en' || forced === 'it' ? forced : resolveLocale(this.settings.ui.language, this.system?.locale));
  }

  /** Persist settings shortly after the last change. */
  scheduleSave() {
    if (!this.loaded) return;
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => {
      api.saveSettings($state.snapshot(this.settings) as Settings).catch((e) =>
        this.toast(tr('Settings not saved: {error}', { error: String(e) }), 'error'),
      );
    }, 350);
  }

  /** Persist now: for backend steps that read the saved settings (the preparation). */
  async saveNow() {
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    this.#saveTimer = null;
    await api.saveSettings($state.snapshot(this.settings) as Settings);
  }

  toast(text: string, tone: Tone = 'info', ms = 4200) {
    const id = ++this.#toastId;
    this.toasts.push({ id, text, tone });
    setTimeout(() => {
      this.toasts = this.toasts.filter((t) => t.id !== id);
    }, ms);
  }

  #listenEngine() {
    on<EngineEvent>('engine://event', (e) => {
      if (e.t === 'hello') {
        this.launch.running = true;
      } else if (e.t === 'perf') {
        this.launch.fps = Number(e.fps ?? 0);
        this.launch.frametimeMs = Number(e.frametime_ms ?? 0);
        this.launch.speed = Number(e.speed ?? 0);
      } else if (e.t === 'setting') {
        applyOverlaySetting(this.settings, String(e.key), Number(e.value));
      }
    });
    on<string>('engine://log', (line) => {
      this.engineLog.push(line);
      if (this.engineLog.length > 800) this.engineLog.splice(0, this.engineLog.length - 800);
    });
    on<{ code: number | null; message?: string }>('engine://exit', (e) => {
      const crashed = e.code !== 0 && e.code !== null;
      this.launch = {
        running: false,
        engine: null,
        pid: null,
        startedAt: null,
        fps: null,
        frametimeMs: null,
        speed: null,
        lastError: crashed ? (e.message ?? tr('The engine closed with code {code}', { code: String(e.code) })) : null,
      };
      if (crashed) this.toast(this.launch.lastError!, 'error', 8000);
    });
  }
}

/** Mirror changes made in the in-game overlay back into the launcher's settings. */
function applyOverlaySetting(s: Settings, key: string, value: number) {
  const filters: Record<number, Settings['image']['upscaler']> = {
    0: 'nearest', 1: 'bilinear', 2: 'bicubic', 4: 'lanczos', 5: 'scaleforce', 6: 'fsr1', 7: 'area',
    12: 'mmpx', 13: 'sgsr', 14: 'sgsr_edge',
  };
  switch (key) {
    case 'scaling_filter':
      if (filters[value]) s.image.upscaler = filters[value];
      break;
    case 'fsr_sharpening_slider':
      s.image.sharpness = Math.max(0, Math.min(100, 100 - value / 2));
      break;
    case 'anti_aliasing':
      s.image.antiAliasing = (['none', 'fxaa', 'smaa'] as const)[value] ?? s.image.antiAliasing;
      break;
    case 'resolution_setup':
      s.image.compatScale = value;
      break;
    case 'frame_gen_multiplier':
      s.smoothness.frameGen = value === 0 ? 'off' : 'lsfg';
      if (value >= 2) s.smoothness.multiplier = value as 2 | 3 | 4;
      break;
    case 'frame_gen_flow_scale':
      s.smoothness.flowScale = value;
      break;
    case 'volume':
      s.audio.volume = value;
      break;
    case 'hud':
      s.smoothness.hud = (['off', 'compact', 'full'] as const)[value] ?? s.smoothness.hud;
      break;
  }
  s.preset = 'personalizzato';
}

export const app = new AppState();
