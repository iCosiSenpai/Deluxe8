import type { Settings } from './types';

export const defaultSettings: Settings = {
  version: 1,
  setupDone: false,
  engine: 'compat',
  preset: 'bilanciato',
  paths: { gameFolder: '', base: '', update: '', dlc: [], prodKeys: '', titleKeys: '', firmware: '' },
  display: { fullscreen: true, vsync: 'fifo', aspect: '16:9', outputHeight: 1440 },
  image: {
    compatScale: 5, // 1.5x (1620p), the "Bilanciato" preset
    renderScale: 100,
    upscaler: 'fsr1',
    sharpness: 60,
    antiAliasing: 'smaa',
    anisotropy: '16x',
    hdr: false,
    hdrPaperWhite: 203,
    hdrPeak: 1000,
  },
  smoothness: { frameGen: 'off', multiplier: 2, flowScale: 0, hud: 'compact', speedLimit: true },
  advanced: {
    gpuAccuracy: 'high',
    asyncShaders: false,
    diskShaderCache: true,
    astcDecode: 'gpu',
    vramMode: 'aggressive',
    cpuAccuracy: 'auto',
  },
  audio: { volume: 80, muteInBackground: true, output: 'auto' },
  controls: { rumble: true, motion: true, players: 1, split60: false },
  online: {
    nickname: '',
    lobbyUrl: 'https://api.ynet-fun.xyz',
    hostPort: 24872,
    maxPlayers: 8,
    password: '',
    roomName: 'Stanza Deluxe8',
    upnp: true,
  },
  system: { language: 'it', region: 'eur' },
  native: { dlss: false },
};

export type PresetId = Exclude<Settings['preset'], 'personalizzato'>;

/** What each preset changes. Everything else keeps the user's value. */
export const presets: Record<PresetId, { label: string; summary: string; apply: (s: Settings) => void }> = {
  qualita: {
    label: 'Qualità',
    summary: 'Massima definizione per schermi 1440p e 4K.',
    apply: (s) => {
      s.image.compatScale = 6;
      s.image.renderScale = 150;
      s.image.antiAliasing = 'smaa';
      s.image.anisotropy = '16x';
      s.advanced.gpuAccuracy = 'high';
    },
  },
  bilanciato: {
    label: 'Bilanciato',
    summary: 'Nitido e stabile a 60 fps su quasi ogni scheda video recente.',
    apply: (s) => {
      s.image.compatScale = 5;
      s.image.renderScale = 100;
      s.image.antiAliasing = 'smaa';
      s.image.anisotropy = '16x';
      s.advanced.gpuAccuracy = 'high';
    },
  },
  prestazioni: {
    label: 'Prestazioni',
    summary: 'Per PC portatili e schede video entry level.',
    apply: (s) => {
      s.image.compatScale = 3;
      s.image.renderScale = 58;
      s.image.antiAliasing = 'fxaa';
      s.image.anisotropy = '4x';
      s.advanced.gpuAccuracy = 'normal';
    },
  },
  competitivo: {
    label: 'Competitivo',
    summary: 'Latenza minima: niente frame generation, V-Sync a bassa latenza.',
    apply: (s) => {
      s.image.compatScale = 3;
      s.image.renderScale = 100;
      s.smoothness.frameGen = 'off';
      s.display.vsync = 'mailbox';
      s.smoothness.hud = 'compact';
    },
  },
};
