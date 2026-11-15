// Shared types between the Svelte UI and the Rust backend (serde camelCase).

import type { LocaleSetting, SystemLocale } from './i18n.svelte';

/** compat = the Emulator engine, native = the recompiled game, online = Ryujinx Nextendo (Nextendo Network). */
export type EngineKind = 'native' | 'compat' | 'online';
export type PackageKind = 'base' | 'update' | 'dlc' | 'unknown';

export interface PackageInfo {
  path: string;
  fileName: string;
  container: 'nsp' | 'nsz' | 'xci' | 'unknown';
  titleId: string;
  kind: PackageKind;
  version: number;
  displayVersion: string | null;
  solid: boolean;
  size: number;
  note: string | null;
}

export interface GameScan {
  folder: string;
  base: PackageInfo | null;
  update: PackageInfo | null;
  dlc: PackageInfo[];
  others: PackageInfo[];
  problems: string[];
}

export interface KeysReport {
  path: string;
  ok: boolean;
  count: number;
  hasHeaderKey: boolean;
  maxMasterKeyRevision: number | null;
  missingRequired: string[];
  titleKeysPath: string | null;
  titleKeysCount: number | null;
}

export interface FirmwareReport {
  path: string;
  ok: boolean;
  ncaCount: number;
  versionHint: string | null;
  isZip: boolean;
}

export interface GpuInfo {
  name: string;
  vendor: 'nvidia' | 'amd' | 'intel' | 'other';
  vramMb: number | null;
  driver: string | null;
}

export interface SystemInfo {
  os: string;
  cpu: string;
  cores: number;
  ramGb: number;
  gpus: GpuInfo[];
  displays: { width: number; height: number; refreshHz: number }[];
  losslessDll: string | null;
  nativeEngine: boolean;
  compatEngine: boolean;
  onlineEngine: boolean;
  dataDir: string;
  /** Desktop language and country, for the "auto" interface language. */
  locale: SystemLocale;
}

export interface Candidates {
  gameFolders: string[];
  keys: string[];
  firmware: string[];
}

export type Upscaler =
  | 'fsr1'
  | 'sgsr'
  | 'sgsr_edge'
  | 'scaleforce'
  | 'lanczos'
  | 'bicubic'
  | 'bilinear'
  | 'area'
  | 'mmpx'
  | 'nearest'
  | 'fsr3'
  | 'dlss'
  | 'dlaa'
  | 'fsr4';

export type FrameGen = 'off' | 'lsfg' | 'fsr3' | 'dlss' | 'builtin';

export interface Settings {
  version: 1;
  setupDone: boolean;
  engine: EngineKind;
  preset: 'qualita' | 'bilanciato' | 'prestazioni' | 'competitivo' | 'personalizzato';
  paths: {
    gameFolder: string;
    base: string;
    update: string;
    dlc: string[];
    prodKeys: string;
    titleKeys: string;
    firmware: string;
  };
  display: {
    fullscreen: boolean;
    vsync: 'fifo' | 'mailbox' | 'immediate' | 'fifo_relaxed';
    aspect: '16:9' | '16:10' | '21:9' | '4:3' | 'stretch';
    outputHeight: 720 | 1080 | 1440 | 2160; // native engine
  };
  image: {
    compatScale: number; // Eden ResolutionSetup index (3 = 1x)
    renderScale: number; // native engine, percent 33..200
    upscaler: Upscaler;
    sharpness: number; // 0..100
    antiAliasing: 'none' | 'fxaa' | 'smaa';
    anisotropy: 'auto' | '2x' | '4x' | '8x' | '16x';
    hdr: boolean;
    hdrPaperWhite: number;
    hdrPeak: number;
  };
  smoothness: {
    frameGen: FrameGen;
    multiplier: 2 | 3 | 4;
    flowScale: number; // 0 = auto
    hud: 'off' | 'compact' | 'full';
    speedLimit: boolean;
  };
  advanced: {
    gpuAccuracy: 'normal' | 'high';
    asyncShaders: boolean;
    diskShaderCache: boolean;
    astcDecode: 'gpu' | 'cpu' | 'async';
    vramMode: 'conservative' | 'aggressive';
    cpuAccuracy: 'auto' | 'accurate' | 'unsafe';
  };
  audio: {
    volume: number;
    muteInBackground: boolean;
    output: 'auto' | 'cubeb' | 'sdl';
  };
  controls: {
    rumble: boolean;
    motion: boolean;
    players: number;
    /** 3-4 player split screen at 60 fps (update 4.0.1; IPS on Compat, build-time patch on Native). */
    split60: boolean;
  };
  online: {
    nickname: string;
    lobbyUrl: string;
    hostPort: number;
    maxPlayers: number;
    password: string;
    roomName: string;
    upnp: boolean;
  };
  system: {
    /** The game's language; "auto" follows the interface language. */
    language: 'auto' | 'it' | 'en' | 'fr' | 'de' | 'es' | 'ja';
    region: 'eur' | 'usa' | 'jpn';
  };
  ui: {
    language: LocaleSetting;
  };
  native: {
    /** Download NVIDIA's DLSS SDK from GitHub during the preparation and build the plugin for this PC (NVIDIA RTX). */
    dlss: boolean;
  };
}

/** What the launcher can download (Sistema > Componenti, and the preparation). */
export interface ComponentsStatus {
  nativeVersion: string;
  nativeEngine: string | null; // folder of the Native engine found on this PC
  nativeDownloadMb: number;
  nativeDownloadable: boolean;
  dlssInstalled: boolean;
  dlssVersion: string;
  onlineVersion: string;
  onlineEngine: string | null; // folder of Ryujinx Nextendo when downloaded
  onlineDownloadMb: number;
  /** A Nextendo Network account is linked in Ryujinx Nextendo (Deluxe8 never reads the account file itself). */
  nextendoSignedIn: boolean;
  dlssDownloadMb: number;
  dlssLicenseUrl: string;
  dlssBuildable: boolean;
}

export interface ComponentDone {
  ok: boolean;
  name: 'native' | 'dlss' | 'online';
  message?: string;
}

export interface RoomInfo {
  id: string;
  name: string;
  description: string;
  owner: string;
  address: string;
  port: number;
  players: number;
  maxPlayers: number;
  hasPassword: boolean;
  game: string;
  gameVersion: string | null;
}

export interface ModInfo {
  name: string;
  path: string;
  enabled: boolean;
  kinds: ('romfs' | 'exefs' | 'cheats')[];
  size: number;
}

export interface EngineEvent {
  t: string;
  [key: string]: unknown;
}

export interface PrepareStep {
  id: string;
  label: string;
  state: 'pending' | 'running' | 'done' | 'skipped' | 'error';
  detail: string | null;
  progress: number | null; // 0..1
}

export interface LaunchState {
  running: boolean;
  engine: EngineKind | null;
  pid: number | null;
  startedAt: number | null;
  fps: number | null;
  frametimeMs: number | null;
  speed: number | null;
  lastError: string | null;
}
