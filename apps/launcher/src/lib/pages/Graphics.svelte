<script lang="ts">
  import { tr } from '../i18n.svelte';
  import { onMount } from 'svelte';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import { compatScales, renderScales } from '../labels';
  import SettingRow from '../components/SettingRow.svelte';
  import Segmented from '../components/Segmented.svelte';
  import Slider from '../components/Slider.svelte';
  import Toggle from '../components/Toggle.svelte';
  import type { Upscaler } from '../types';

  const s = app.settings;
  const custom = () => (s.preset = 'personalizzato');

  const gpu = $derived(app.system?.gpus[0] ?? null);
  const nvidiaRtx = $derived(!!gpu && gpu.vendor === 'nvidia' && /RTX/i.test(gpu.name));
  const amdRdna = $derived(!!gpu && gpu.vendor === 'amd' && /RX\s*(6|7|9)\d\d\d/i.test(gpu.name));
  const native = $derived(s.engine === 'native');
  const lossless = $derived(!!app.system?.losslessDll);
  let dlssInstalled = $state(true);
  onMount(() => {
    api.componentsStatus().then((c) => (dlssInstalled = c.dlssInstalled)).catch(() => {});
  });
  const wantsDlss = $derived(
    native && (s.image.upscaler === 'dlss' || s.image.upscaler === 'dlaa' || s.smoothness.frameGen === 'dlss'),
  );

  const compatUpscalers: { value: Upscaler; label: string; recommended?: boolean }[] = $derived([
    { value: 'fsr1', label: 'FSR 1', recommended: true },
    { value: 'sgsr', label: 'SGSR' },
    { value: 'scaleforce', label: 'ScaleForce' },
    { value: 'lanczos', label: 'Lanczos' },
    { value: 'bicubic', label: tr('Bicubic') },
    { value: 'area', label: 'Area' },
    { value: 'mmpx', label: 'MMPX' },
    { value: 'nearest', label: tr('Sharp pixels') },
  ]);
  const nativeUpscalers = $derived([
    { value: 'dlss' as Upscaler, label: 'DLSS', disabled: !nvidiaRtx, recommended: nvidiaRtx },
    { value: 'dlaa' as Upscaler, label: 'DLAA', disabled: !nvidiaRtx },
    { value: 'fsr3' as Upscaler, label: 'FSR 3.1', recommended: !nvidiaRtx },
    { value: 'fsr4' as Upscaler, label: 'FSR 4', disabled: !amdRdna },
    { value: 'fsr1' as Upscaler, label: 'FSR 1' },
  ]);

  // The Native engine's HDR menu choices (nits): the launcher's values must be among them.
  const hdrPaperWhites = [100, 150, 203, 250, 300];
  const hdrPeaks = [400, 600, 800, 1000, 1500];
  const nearest = (choices: number[], v: number) =>
    choices.reduce((a, b) => (Math.abs(b - v) < Math.abs(a - v) ? b : a));

  // Keep the selection valid when the engine changes.
  $effect(() => {
    if (!hdrPaperWhites.includes(s.image.hdrPaperWhite)) s.image.hdrPaperWhite = nearest(hdrPaperWhites, s.image.hdrPaperWhite);
    if (!hdrPeaks.includes(s.image.hdrPeak)) s.image.hdrPeak = nearest(hdrPeaks, s.image.hdrPeak);
    // the Native engine offers fixed scene scales: an older free value goes to the nearest one
    if (!renderScales.some((r) => r.value === s.image.renderScale)) {
      s.image.renderScale = renderScales.reduce((a, b) =>
        Math.abs(b.value - s.image.renderScale) < Math.abs(a.value - s.image.renderScale) ? b : a,
      ).value;
    }
    const valid = native ? nativeUpscalers.filter((u) => !u.disabled).map((u) => u.value) : compatUpscalers.map((u) => u.value);
    if (!valid.includes(s.image.upscaler)) s.image.upscaler = valid[0];
    if (!native && (s.smoothness.frameGen === 'fsr3' || s.smoothness.frameGen === 'dlss' || s.smoothness.frameGen === 'builtin'))
      s.smoothness.frameGen = 'off';
    if (native && s.smoothness.frameGen === 'lsfg') s.smoothness.frameGen = 'off';
  });

  // English source text, shown through tr
  const upscalerHelp: Record<string, string> = {
    fsr1: 'AMD\'s spatial upscaler with RCAS sharpening. Works on any graphics card, NVIDIA included.',
    sgsr: 'Snapdragon Game Super Resolution: spatial, very light, clean edges.',
    scaleforce: 'An adaptive filter made for emulation: soft but without halos.',
    lanczos: 'Classic resampling: sharp, may add light halos.',
    bicubic: 'Soft and even.',
    area: 'Best when the internal resolution is higher than the screen (supersampling).',
    mmpx: 'Made for pixel art: of little use here.',
    nearest: 'No filtering: hard pixels, for whole multiples only.',
    dlss: 'NVIDIA DLSS Super Resolution with the game\'s real motion vectors; the driver picks the model (transformer on recent RTX cards).',
    dlaa: 'DLSS at native resolution: the best temporal anti-aliasing on RTX cards.',
    fsr3: 'AMD FSR 3.1, temporal and open source; also works on NVIDIA and Intel.',
    fsr4: 'AMD FSR 4, machine-learning upscaling, for Radeon RX 6000, 7000 and 9000.',
  };
</script>

<section class="page">
  <h1 class="display">{tr('Graphics')}</h1>
  <p class="lead">
    {native
      ? tr('Native engine: the game draws straight to Vulkan at the resolution you choose.')
      : tr('Emulator engine: the scene is rendered larger, then fitted to your screen.')}
  </p>

  <h2>{tr('Resolution')}</h2>
  <div class="sheet">
    {#if native}
      <SettingRow label={tr('Output resolution')} description={tr('The final image, HUD included, always sharp.')} cost={2}>
        <Segmented
          label={tr('Output resolution')}
          bind:value={s.display.outputHeight}
          onchange={custom}
          options={[
            { value: 720, label: '720p' },
            { value: 1080, label: '1080p' },
            { value: 1440, label: '1440p', recommended: true },
            { value: 2160, label: '4K' },
          ]}
        />
      </SettingRow>
      <SettingRow
        label={tr('Scene scale')}
        description={tr(renderScales.find((r) => r.value === s.image.renderScale)?.help ?? '')}
        cost={s.image.renderScale > 120 ? 3 : s.image.renderScale > 90 ? 2 : 1}
        stacked
      >
        <Segmented
          label={tr('Scene scale')}
          bind:value={s.image.renderScale}
          onchange={custom}
          options={renderScales.map((r) => ({ value: r.value, label: r.label, recommended: r.value === 67 }))}
        />
      </SettingRow>
    {:else}
      <SettingRow
        label={tr('Internal resolution')}
        description={tr("A multiple of the game's TV-mode resolution (1080p). 2x equals 4K.")}
        cost={s.image.compatScale >= 7 ? 3 : s.image.compatScale >= 5 ? 2 : 1}
        stacked
      >
        <Segmented
          label={tr('Internal resolution')}
          bind:value={s.image.compatScale}
          onchange={custom}
          options={compatScales.map((c) => ({ value: c.value, label: tr(c.label), recommended: c.value === 6 }))}
        />
      </SettingRow>
    {/if}
  </div>

  <h2>{tr('Upscaling and sharpness')}</h2>
  <div class="sheet">
    <SettingRow label="Upscaler" description={upscalerHelp[s.image.upscaler] ? tr(upscalerHelp[s.image.upscaler]) : ''} stacked>
      <Segmented
        label="Upscaler"
        bind:value={s.image.upscaler}
        onchange={custom}
        options={native ? nativeUpscalers : compatUpscalers}
      />
    </SettingRow>
    {#if native && !nvidiaRtx}
      <p class="aside">{tr('DLSS and DLAA need an NVIDIA GeForce RTX.')}</p>
    {/if}
    {#if native && nvidiaRtx}
      <p class="aside">{tr('FSR 4 is not available on NVIDIA: AMD limits it to Radeon cards and falls back to FSR 3.1 by itself.')}</p>
    {/if}
    {#if wantsDlss && !dlssInstalled}
      <p class="aside warn">
        {tr('DLSS is not on this PC yet: download it from System > Components (or prepare the game again). Until then the game uses another upscaler.')}
      </p>
    {/if}
    <SettingRow label={tr('Sharpness')} description={tr('Local contrast applied after upscaling.')} cost={1}>
      <Slider label={tr('Sharpness')} bind:value={s.image.sharpness} min={0} max={100} step={5} format={(v) => `${v}%`} onchange={custom} />
    </SettingRow>
    {#if !native}
      <SettingRow label="Anti-aliasing" description={tr('SMAA is more precise, FXAA lighter.')} cost={1}>
        <Segmented
          label="Anti-aliasing"
          bind:value={s.image.antiAliasing}
          onchange={custom}
          options={[
            { value: 'none', label: tr('aa|None') },
            { value: 'fxaa', label: 'FXAA' },
            { value: 'smaa', label: 'SMAA', recommended: true },
          ]}
        />
      </SettingRow>
    {/if}
    <SettingRow label={tr('Anisotropic filtering')} description={tr('Sharper track textures in the distance.')} cost={1}>
      <Segmented
        label={tr('Anisotropic filtering')}
        bind:value={s.image.anisotropy}
        onchange={custom}
        options={[
          { value: 'auto', label: 'Auto' },
          { value: '4x', label: '4x' },
          { value: '8x', label: '8x' },
          { value: '16x', label: '16x', recommended: true },
        ]}
      />
    </SettingRow>
  </div>

  <h2>{tr('Smoothness')}</h2>
  <div class="sheet">
    <SettingRow
      label="Frame generation"
      description={tr('Adds in-between frames: the game stays at 60, the screen shows more. Adds a little latency.')}
      tag={s.smoothness.frameGen === 'off' ? null : tr('Latency +')}
      tone="giallo"
      stacked
    >
      <Segmented
        label="Frame generation"
        bind:value={s.smoothness.frameGen}
        onchange={custom}
        options={native
          ? [
              { value: 'off', label: tr('framegen|Off') },
              { value: 'dlss', label: 'DLSS (2x)', disabled: !nvidiaRtx },
              { value: 'fsr3', label: 'FSR 3 (2x)', recommended: !nvidiaRtx },
              { value: 'builtin', label: tr('Built-in') },
            ]
          : [
              { value: 'off', label: tr('framegen|Off') },
              { value: 'lsfg', label: 'Lossless Scaling', disabled: !lossless },
            ]}
      />
    </SettingRow>
    {#if !native && !lossless}
      <p class="aside">
        {tr("Frame generation in the Emulator needs Lossless Scaling (Steam): Deluxe8 uses your own Lossless.dll and keeps it on this PC.")}
      </p>
    {/if}
    {#if s.smoothness.frameGen === 'lsfg' || (native && s.smoothness.frameGen === 'builtin')}
      <SettingRow label={tr('Multiplier')} description={tr('How many frames on screen for each frame of the game.')}>
        <Segmented
          label={tr('Multiplier')}
          bind:value={s.smoothness.multiplier}
          onchange={custom}
          options={[
            { value: 2, label: '2x', recommended: true },
            { value: 3, label: '3x' },
            { value: 4, label: '4x' },
          ]}
        />
      </SettingRow>
    {/if}
    {#if s.smoothness.frameGen === 'lsfg'}
      <SettingRow label={tr('Flow scale')} description={tr('Precision of the motion estimate. 0 lets Deluxe8 decide.')}>
        <Slider label={tr('Flow scale')} bind:value={s.smoothness.flowScale} min={0} max={100} step={5} format={(v) => (v === 0 ? 'Auto' : `${v}%`)} onchange={custom} />
      </SettingRow>
    {/if}
    {#if nvidiaRtx}
      <p class="aside">
        {tr('A zero-setup alternative: NVIDIA Smooth Motion (RTX 40/50) generates frames in the driver, Vulkan games included. Turn it on per program in the NVIDIA app.')}
      </p>
    {/if}
    <SettingRow label={tr('Vertical sync')} description={tr('FIFO removes tearing; Mailbox lowers latency on screens above 60 Hz.')}>
      <Segmented
        label="V-Sync"
        bind:value={s.display.vsync}
        onchange={custom}
        options={[
          { value: 'fifo', label: 'FIFO', recommended: true },
          { value: 'mailbox', label: 'Mailbox' },
          { value: 'fifo_relaxed', label: tr('Relaxed FIFO') },
          { value: 'immediate', label: tr('vsync|Off') },
        ]}
      />
    </SettingRow>
    <SettingRow label={tr('Performance overlay')} description={tr('FPS and frame times in game. Also changes from the L3 + R3 menu.')}>
      <Segmented
        label={tr('Performance overlay')}
        bind:value={s.smoothness.hud}
        options={[
          { value: 'off', label: tr('Off') },
          { value: 'compact', label: tr('Essential') },
          { value: 'full', label: tr('Full') },
        ]}
      />
    </SettingRow>
  </div>

  {#if native}
    <h2>HDR</h2>
    <div class="sheet">
      <SettingRow label="HDR10" description={tr("Needs the screen in HDR mode. Tone mapping comes from the game's own HDR scene.")}>
        <Toggle label="HDR10" bind:checked={s.image.hdr} onchange={custom} />
      </SettingRow>
      {#if s.image.hdr}
        <SettingRow label={tr('Paper white')} description={tr('Brightness of the HUD and menus, in nits.')} stacked>
          <Segmented
            label={tr('Paper white')}
            bind:value={s.image.hdrPaperWhite}
            onchange={custom}
            options={hdrPaperWhites.map((n) => ({ value: n, label: `${n} nit`, recommended: n === 203 }))}
          />
        </SettingRow>
        <SettingRow label={tr('Peak')} description={tr("Your screen's maximum brightness, in nits.")} stacked>
          <Segmented
            label={tr('Peak')}
            bind:value={s.image.hdrPeak}
            onchange={custom}
            options={hdrPeaks.map((n) => ({ value: n, label: `${n} nit` }))}
          />
        </SettingRow>
      {/if}
    </div>
  {/if}

  <h2>{tr('Screen')}</h2>
  <div class="sheet">
    <SettingRow label={tr('Full screen')} description={tr('Borderless on the main monitor.')}>
      <Toggle label={tr('Full screen')} bind:checked={s.display.fullscreen} />
    </SettingRow>
    {#if !native}
      <SettingRow label={tr('Aspect ratio')} description={tr('The game is drawn in 16:9: other ratios stretch it.')}>
        <Segmented
          label={tr('Aspect ratio')}
          bind:value={s.display.aspect}
          options={[
            { value: '16:9', label: '16:9', recommended: true },
            { value: '16:10', label: '16:10' },
            { value: '21:9', label: '21:9' },
            { value: 'stretch', label: tr('Stretch') },
          ]}
        />
      </SettingRow>
    {/if}
  </div>

  <h2>{tr('Advanced')}</h2>
  <div class="sheet">
    {#if !native}
      <SettingRow label={tr('GPU accuracy')} description={tr('Accurate is the reference setting; Fast gains a few frames on slower PCs, with rare glitches.')} cost={s.advanced.gpuAccuracy === 'high' ? 2 : 1}>
        <Segmented
          label={tr('GPU accuracy')}
          bind:value={s.advanced.gpuAccuracy}
          onchange={custom}
          options={[
            { value: 'high', label: tr('Accurate'), recommended: true },
            { value: 'normal', label: tr('Fast') },
          ]}
        />
      </SettingRow>
      <SettingRow label={tr('Asynchronous shaders')} description={tr('Fewer stutters the first time an effect shows up, at the cost of briefly missing details.')}>
        <Toggle label={tr('Asynchronous shaders')} bind:checked={s.advanced.asyncShaders} onchange={custom} />
      </SettingRow>
      <SettingRow label={tr('ASTC texture decoding')} description={tr('Fastest on the GPU; asynchronous on the CPU is the smoothest on first load.')}>
        <Segmented
          label={tr('ASTC texture decoding')}
          bind:value={s.advanced.astcDecode}
          onchange={custom}
          options={[
            { value: 'gpu', label: 'GPU', recommended: true },
            { value: 'async', label: tr('Asynchronous CPU') },
            { value: 'cpu', label: 'CPU' },
          ]}
        />
      </SettingRow>
      <SettingRow label={tr('VRAM use')} description={tr('Aggressive keeps more in video memory: fewer loads, more VRAM in use.')}>
        <Segmented
          label={tr('VRAM use')}
          bind:value={s.advanced.vramMode}
          onchange={custom}
          options={[
            { value: 'conservative', label: tr('Conservative') },
            { value: 'aggressive', label: tr('Aggressive'), recommended: (gpu?.vramMb ?? 0) >= 8000 },
          ]}
        />
      </SettingRow>
    {/if}
    <SettingRow label={tr('Shader cache on disk')} description={tr('Keeps compiled shaders: no stutters from the second race on.')}>
      <Toggle label={tr('Shader cache on disk')} bind:checked={s.advanced.diskShaderCache} />
    </SettingRow>
  </div>
</section>

<style>
  .page {
    padding: var(--s6) var(--s7) var(--s7);
    max-width: 1120px;
  }
  h1 {
    font-size: var(--fs-3xl);
    margin-bottom: var(--s3);
  }
  .lead {
    color: var(--gesso-2);
    margin-bottom: var(--s5);
    max-width: 70ch;
  }
  h2 {
    font-size: var(--fs-lg);
    font-weight: 760;
    margin: var(--s6) 0 var(--s2);
  }
  .sheet {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: 0 var(--s5);
  }
  .aside {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    padding: var(--s3) 0;
    border-bottom: 1px solid var(--linea);
    max-width: 80ch;
  }
  .aside.warn {
    color: var(--giallo);
  }
</style>
