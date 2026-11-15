// Thin typed layer over Tauri commands/events. In a plain browser (vite dev preview) every call
// is served by in-memory mocks so the UI can be designed and reviewed without the backend.

import type {
  Candidates,
  ComponentsStatus,
  EngineEvent,
  EngineKind,
  FirmwareReport,
  GameScan,
  KeysReport,
  ModInfo,
  PrepareStep,
  RoomInfo,
  Settings,
  SystemInfo,
} from './types';
import { defaultSettings } from './defaults';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

type Unlisten = () => void;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }
  return mock(cmd, args) as Promise<T>;
}

export async function on<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (isTauri) {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<T>(event, (e) => handler(e.payload));
  }
  const fn = (e: Event) => handler((e as CustomEvent<T>).detail);
  window.addEventListener(event, fn);
  return () => window.removeEventListener(event, fn);
}

export async function pickPath(opts: {
  directory?: boolean;
  title: string;
  filters?: { name: string; extensions: string[] }[];
  defaultPath?: string;
}): Promise<string | null> {
  if (!isTauri) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const result = await open({
    directory: opts.directory ?? false,
    multiple: false,
    title: opts.title,
    filters: opts.filters,
    defaultPath: opts.defaultPath,
  });
  return typeof result === 'string' ? result : null;
}

/** Opens a web page in the default browser (licence pages of downloaded components). */
export async function openUrl(url: string): Promise<void> {
  if (!isTauri) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  const { openUrl: open } = await import('@tauri-apps/plugin-opener');
  await open(url);
}

export const api = {
  getSettings: () => call<Settings>('get_settings'),
  saveSettings: (settings: Settings) => call<void>('save_settings', { settings }),
  systemInfo: () => call<SystemInfo>('system_info'),
  findCandidates: () => call<Candidates>('find_candidates'),
  scanGameFolder: (path: string) => call<GameScan>('scan_game_folder', { path }),
  inspectKeys: (path: string) => call<KeysReport>('inspect_keys', { path }),
  inspectFirmware: (path: string) => call<FirmwareReport>('inspect_firmware', { path }),
  prepare: (engine: EngineKind) => call<void>('prepare', { engine }),
  cancelPrepare: () => call<void>('cancel_prepare'),
  componentsStatus: () => call<ComponentsStatus>('components_status'),
  /** Progress as `prepare://step` events, the end as `component://done`. */
  installComponent: (name: 'native' | 'dlss') => call<void>('install_component', { name }),
  launch: (engine: EngineKind, room?: { address: string; port: number; password: string }) =>
    call<void>('launch', { engine, room: room ?? null }),
  stopEngine: () => call<void>('stop_engine'),
  listRooms: () => call<RoomInfo[]>('list_rooms'),
  hostRoom: () => call<{ address: string; port: number; upnp: string }>('host_room'),
  stopRoom: () => call<void>('stop_room'),
  roomStatus: () => call<{ hosting: boolean; address: string | null; port: number | null }>('room_status'),
  listMods: () => call<ModInfo[]>('list_mods'),
  setModEnabled: (name: string, enabled: boolean) => call<void>('set_mod_enabled', { name, enabled }),
  modsFolder: () => call<string>('mods_folder'),
  /** Opens one of Deluxe8's own folders in the file manager (the webview cannot open arbitrary paths). */
  openFolder: (which: 'mods' | 'logs' | 'data') => call<void>('open_folder', { which }),
  readLog: (which: 'engine' | 'launcher') => call<string>('read_log', { which }),
};

// ------------------------------------------------------------------------------------------
// Browser mocks

/**
 * Browser preview only: `?page=grafica&engine=native` opens a page as after the setup (for reviewing the UI and taking the
 * README screenshots). Never read inside the Tauri app.
 */
export const demo: URLSearchParams | null =
  !isTauri && typeof location !== 'undefined' && new URLSearchParams(location.search).has('page')
    ? new URLSearchParams(location.search)
    : null;

let mockSettings: Settings = structuredClone(defaultSettings);
let mockNative = demo?.get('engine') === 'native';
let mockDlss = mockNative;
if (demo) {
  mockSettings.setupDone = true;
  mockSettings.paths.dlc = ['D:\\Giochi\\Switch\\Mario Kart 8 Deluxe\\Booster Course Pass.nsp'];
  mockSettings.online.nickname = 'Lupo 8';
  if (mockNative) {
    mockSettings.engine = 'native';
    mockSettings.native.dlss = true;
    mockSettings.image.upscaler = 'dlss';
    mockSettings.image.renderScale = 67;
    mockSettings.smoothness.frameGen = 'dlss';
  }
}
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const emit = (event: string, detail: unknown) =>
  window.dispatchEvent(new CustomEvent(event, { detail }));

async function mock(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  await delay(120);
  switch (cmd) {
    case 'get_settings':
      return structuredClone(mockSettings);
    case 'save_settings':
      mockSettings = structuredClone(args!.settings as Settings);
      return;
    case 'system_info':
      return {
        os: 'Windows 11 Pro 26300',
        cpu: 'Intel Core Ultra 7 265K',
        cores: 20,
        ramGb: 64,
        gpus: [{ name: 'NVIDIA GeForce RTX 5070 Ti', vendor: 'nvidia', vramMb: 16303, driver: '621.01' }],
        displays: [{ width: 2560, height: 1440, refreshHz: 240 }],
        losslessDll: 'C:\\Steam\\steamapps\\common\\Lossless Scaling\\Lossless.dll',
        nativeEngine: mockNative,
        compatEngine: true,
        dataDir: 'C:\\Users\\Giocatore\\AppData\\Local\\Deluxe8\\data',
      } satisfies SystemInfo;
    case 'find_candidates':
      return {
        gameFolders: ['D:\\Giochi\\Switch\\Mario Kart 8 Deluxe'],
        keys: ['D:\\Giochi\\Switch\\Chiavi\\prod.keys'],
        firmware: ['D:\\Giochi\\Switch\\Firmware.22.5.0'],
      } satisfies Candidates;
    case 'scan_game_folder': {
      const folder = args!.path as string;
      const pkg = (kind: 'base' | 'update' | 'dlc', file: string, tid: string, version: number,
        dv: string | null, size: number, solid: boolean) => ({
        path: `${folder}\\${file}`, fileName: file, container: (file.endsWith('.nsz') ? 'nsz' : 'nsp') as 'nsz' | 'nsp',
        titleId: tid, kind, version, displayVersion: dv, solid, size,
        note: solid ? 'Compressione solid: verrà convertito in NSP una volta sola' : null,
      });
      return {
        folder,
        base: pkg('base', 'MK8D [0100152000022000][v0].nsz', '0100152000022000', 0, '1.0.0', 4907846869, true),
        update: pkg('update', 'MK8D [0100152000022800][v1507328].nsz', '0100152000022800', 1507328, '4.0.1', 3770194474, false),
        dlc: [pkg('dlc', 'MK8D Booster Course Pass [0100152000023001].nsp', '0100152000023001', 65536, null, 121034, false)],
        others: [],
        problems: [],
      } satisfies GameScan;
    }
    case 'inspect_keys':
      return {
        path: args!.path as string, ok: true, count: 212, hasHeaderKey: true,
        maxMasterKeyRevision: 0x14, missingRequired: [],
        titleKeysPath: (args!.path as string).replace('prod.keys', 'title.keys'), titleKeysCount: 4,
      } satisfies KeysReport;
    case 'inspect_firmware':
      return { path: args!.path as string, ok: true, ncaCount: 248, versionHint: '22.5.0', isZip: false } satisfies FirmwareReport;
    case 'prepare': {
      const steps: PrepareStep[] = [
        { id: 'keys', label: 'Copia delle chiavi', state: 'pending', detail: null, progress: null },
        { id: 'firmware', label: 'Installazione del firmware', state: 'pending', detail: null, progress: null },
        { id: 'base', label: 'Conversione del gioco base in NSP', state: 'pending', detail: null, progress: null },
        { id: 'config', label: 'Configurazione del motore', state: 'pending', detail: null, progress: null },
      ];
      (async () => {
        for (const s of steps) {
          s.state = 'running';
          for (let p = 0; p <= 1.0001; p += 0.25) {
            s.progress = p;
            emit('prepare://step', structuredClone(s));
            await delay(220);
          }
          s.state = 'done';
          s.progress = 1;
          emit('prepare://step', structuredClone(s));
        }
        emit('prepare://done', { ok: true });
      })();
      return;
    }
    case 'components_status':
      return {
        nativeVersion: '0.2.4',
        nativeEngine: mockNative ? 'C:\\Users\\Giocatore\\AppData\\Local\\Deluxe8\\data\\engines\\native\\v0.2.4' : null,
        nativeDownloadMb: 213,
        nativeDownloadable: true,
        dlssInstalled: mockDlss,
        dlssVersion: '310.9.16',
        dlssDownloadMb: 73,
        dlssLicenseUrl: 'https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt',
        dlssBuildable: true,
      } satisfies ComponentsStatus;
    case 'install_component': {
      const name = args!.name as 'native' | 'dlss';
      const step = name === 'native'
        ? { id: 'native-engine', label: 'Motore Nativo' }
        : { id: 'dlss', label: 'NVIDIA DLSS per questo PC' };
      (async () => {
        for (let p = 0; p <= 1.0001; p += 0.1) {
          emit('prepare://step', { ...step, state: 'running', detail: `Download da GitHub: ${Math.round(p * 100)}%`, progress: p });
          await delay(180);
        }
        emit('prepare://step', { ...step, state: 'done', detail: 'Pronto', progress: 1 });
        if (name === 'native') mockNative = true;
        else mockDlss = true;
        emit('component://done', { ok: true, name });
      })();
      return;
    }
    case 'launch':
      (async () => {
        emit('engine://event', { t: 'hello', engine: 'deluxe8-compat' } satisfies EngineEvent);
        await delay(800);
        emit('engine://event', { t: 'running', program_id: '0100152000022000' });
        for (let i = 0; i < 20; i++) {
          await delay(500);
          emit('engine://event', { t: 'perf', fps: 59.9 + Math.random() * 0.1, frametime_ms: 16.6, speed: 100 });
        }
        emit('engine://exit', { code: 0 });
      })();
      return;
    case 'stop_engine':
      emit('engine://exit', { code: 0 });
      return;
    case 'list_rooms':
      return [
        { id: '1', name: 'Gran Premio del giovedì', description: 'Solo 150cc, niente oggetti blu', owner: 'Lupo', address: '203.0.113.10', port: 24872, players: 5, maxPlayers: 8, hasPassword: false, game: 'Mario Kart 8 Deluxe', gameVersion: '4.0.1' },
        { id: '2', name: 'Booster Course Pass tour', description: '', owner: 'Mirtilla', address: '198.51.100.4', port: 24872, players: 2, maxPlayers: 8, hasPassword: true, game: 'Mario Kart 8 Deluxe', gameVersion: '4.0.1' },
        { id: '3', name: 'Vecchia versione', description: 'Gruppo fermo alla 3.0.4', owner: 'Gigi', address: '192.0.2.77', port: 24872, players: 3, maxPlayers: 8, hasPassword: false, game: 'Mario Kart 8 Deluxe', gameVersion: '3.0.4' },
      ] satisfies RoomInfo[];
    case 'host_room':
      return { address: '192.168.1.20', port: 24872, upnp: 'Porta aperta sul router (UPnP)' };
    case 'room_status':
      return { hosting: false, address: null, port: null };
    case 'list_mods':
      return [
        { name: 'Kart HD texture', path: 'mods/Kart HD texture', enabled: true, kinds: ['romfs'], size: 812_000_000 },
        { name: 'Interfaccia in stile classico', path: 'mods/UI', enabled: false, kinds: ['romfs'], size: 24_000_000 },
      ] satisfies ModInfo[];
    case 'mods_folder':
      return 'C:\\Users\\Giocatore\\AppData\\Local\\Deluxe8\\data\\compat\\load\\0100152000022000';
    case 'open_folder':
      return;
    case 'read_log':
      return '[ 0.000] Deluxe8 launcher avviato\n[ 0.120] Motore Compat trovato\n';
    default:
      return;
  }
}
