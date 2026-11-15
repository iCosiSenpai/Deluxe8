import { tr } from './i18n.svelte';
import type { FrameGen, Upscaler } from './types';

/** Eden ResolutionSetup index -> multiplier of the game's 1080p docked image. */
export const compatScales: { value: number; label: string; height: number }[] = [
  { value: 1, label: '0.5x', height: 540 },
  { value: 2, label: '0.75x', height: 810 },
  { value: 3, label: '1x', height: 1080 },
  { value: 4, label: '1.25x', height: 1350 },
  { value: 5, label: '1.5x', height: 1620 },
  { value: 6, label: '2x', height: 2160 },
  { value: 7, label: '3x', height: 3240 },
  { value: 8, label: '4x', height: 4320 },
  { value: 10, label: '6x', height: 6480 },
  { value: 12, label: '8x', height: 8640 },
];

/** Scene scales the Native engine offers (its "3D render scale" row): below 100% the upscaler rebuilds the image.
 * `help` is English source text (shown through `tr`). */
export const renderScales: { value: number; label: string; help: string }[] = [
  { value: 33, label: '33%', help: 'Ultra performance: a third of the resolution, rebuilt by the upscaler. For 4K screens and modest cards.' },
  { value: 50, label: '50%', help: 'Performance: half the resolution, rebuilt by the upscaler.' },
  { value: 58, label: '58%', help: 'Balanced: a good compromise between sharpness and smoothness.' },
  { value: 67, label: '67%', help: 'Quality: almost indistinguishable from 100% with DLSS or FSR 3.1.' },
  { value: 100, label: '100%', help: 'Native resolution: the upscaler only does anti-aliasing (DLAA with DLSS).' },
  { value: 150, label: '150%', help: 'Supersampling: the scene is drawn larger, then scaled down. Heavy.' },
  { value: 200, label: '200%', help: 'Supersampling 2x: the sharpest image, for very powerful cards only.' },
];

export function scaleLabel(index: number): string {
  const s = compatScales.find((x) => x.value === index);
  return s ? `${tr(s.label)} (${s.height}p)` : `${index}`;
}

export const upscalerNames: Record<Upscaler, string> = {
  fsr1: 'AMD FSR 1',
  sgsr: 'Snapdragon GSR',
  sgsr_edge: 'Snapdragon GSR (edges)',
  scaleforce: 'ScaleForce',
  lanczos: 'Lanczos',
  bicubic: 'Bicubic',
  bilinear: 'Bilinear',
  area: 'Area',
  mmpx: 'MMPX',
  nearest: 'Sharp pixels',
  fsr3: 'AMD FSR 3.1',
  dlss: 'NVIDIA DLSS',
  dlaa: 'NVIDIA DLAA',
  fsr4: 'AMD FSR 4',
};

export function upscalerLabel(u: Upscaler): string {
  return upscalerNames[u] ? tr(upscalerNames[u]) : u;
}

export function frameGenLabel(f: FrameGen, multiplier: number): string {
  switch (f) {
    case 'off':
      return tr('framegen|Off');
    case 'lsfg':
      return `Lossless Scaling ${multiplier}x`;
    case 'fsr3':
      return 'AMD FSR 3 (2x)';
    case 'dlss':
      return 'NVIDIA DLSS (2x)';
    case 'builtin':
      return tr('Built-in interpolator {n}x', { n: multiplier });
  }
}
