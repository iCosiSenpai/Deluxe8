<script lang="ts">
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

  const compatUpscalers: { value: Upscaler; label: string; recommended?: boolean }[] = [
    { value: 'fsr1', label: 'FSR 1', recommended: true },
    { value: 'sgsr', label: 'SGSR' },
    { value: 'scaleforce', label: 'ScaleForce' },
    { value: 'lanczos', label: 'Lanczos' },
    { value: 'bicubic', label: 'Bicubico' },
    { value: 'area', label: 'Area' },
    { value: 'mmpx', label: 'MMPX' },
    { value: 'nearest', label: 'Pixel nitidi' },
  ];
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

  const upscalerHelp: Record<string, string> = {
    fsr1: 'Upscaling spaziale di AMD con nitidezza RCAS. Funziona su qualsiasi scheda video, anche NVIDIA.',
    sgsr: 'Snapdragon Game Super Resolution: spaziale, molto leggero, bordi puliti.',
    scaleforce: 'Filtro adattivo pensato per l\'emulazione: morbido ma senza aloni.',
    lanczos: 'Ricampionamento classico, nitido, può creare leggeri aloni.',
    bicubic: 'Morbido e uniforme.',
    area: 'Ideale quando la risoluzione interna è più alta dello schermo (supersampling).',
    mmpx: 'Pensato per la pixel art: qui serve poco.',
    nearest: 'Nessun filtro: pixel netti, adatto solo a multipli interi.',
    dlss: 'NVIDIA DLSS Super Resolution con i vettori di movimento reali del gioco; il modello lo sceglie il driver (transformer sulle RTX recenti).',
    dlaa: 'DLSS alla risoluzione nativa: la migliore anti-aliasing temporale disponibile su RTX.',
    fsr3: 'AMD FSR 3.1 temporale, open source, funziona anche su NVIDIA e Intel.',
    fsr4: 'AMD FSR 4, upscaling con machine learning, per le Radeon RX 6000, 7000 e 9000.',
  };
</script>

<section class="page">
  <h1 class="display">Grafica</h1>
  <p class="lead">
    {native
      ? 'Motore Nativo: il gioco disegna direttamente in Vulkan alla risoluzione che scegli.'
      : 'Motore Compat: la scena viene renderizzata più grande e poi adattata al tuo schermo.'}
  </p>

  <h2>Risoluzione</h2>
  <div class="sheet">
    {#if native}
      <SettingRow label="Risoluzione di uscita" description="L'immagine finale, HUD compreso, sempre nitida." cost={2}>
        <Segmented
          label="Risoluzione di uscita"
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
        label="Scala della scena"
        description={renderScales.find((r) => r.value === s.image.renderScale)?.help ?? ''}
        cost={s.image.renderScale > 120 ? 3 : s.image.renderScale > 90 ? 2 : 1}
        stacked
      >
        <Segmented
          label="Scala della scena"
          bind:value={s.image.renderScale}
          onchange={custom}
          options={renderScales.map((r) => ({ value: r.value, label: r.label, recommended: r.value === 67 }))}
        />
      </SettingRow>
    {:else}
      <SettingRow
        label="Risoluzione interna"
        description="Multiplo della risoluzione del gioco in modalità TV (1080p). 2x equivale al 4K."
        cost={s.image.compatScale >= 7 ? 3 : s.image.compatScale >= 5 ? 2 : 1}
        stacked
      >
        <Segmented
          label="Risoluzione interna"
          bind:value={s.image.compatScale}
          onchange={custom}
          options={compatScales.map((c) => ({ value: c.value, label: c.label, recommended: c.value === 6 }))}
        />
      </SettingRow>
    {/if}
  </div>

  <h2>Upscaling e nitidezza</h2>
  <div class="sheet">
    <SettingRow label="Upscaler" description={upscalerHelp[s.image.upscaler] ?? ''} stacked>
      <Segmented
        label="Upscaler"
        bind:value={s.image.upscaler}
        onchange={custom}
        options={native ? nativeUpscalers : compatUpscalers}
      />
    </SettingRow>
    {#if native && !nvidiaRtx}
      <p class="aside">DLSS e DLAA richiedono una NVIDIA GeForce RTX.</p>
    {/if}
    {#if native && nvidiaRtx}
      <p class="aside">FSR 4 non è disponibile su NVIDIA: AMD lo limita alle Radeon e passa automaticamente a FSR 3.1.</p>
    {/if}
    {#if wantsDlss && !dlssInstalled}
      <p class="aside warn">
        DLSS non è ancora su questo PC: scaricalo da Sistema &gt; Componenti (o rifai la preparazione). Fino ad allora il gioco
        usa un altro upscaler.
      </p>
    {/if}
    <SettingRow label="Nitidezza" description="Contrasto locale applicato dopo l'upscaling." cost={1}>
      <Slider label="Nitidezza" bind:value={s.image.sharpness} min={0} max={100} step={5} format={(v) => `${v}%`} onchange={custom} />
    </SettingRow>
    {#if !native}
      <SettingRow label="Anti-aliasing" description="SMAA è più preciso, FXAA più leggero." cost={1}>
        <Segmented
          label="Anti-aliasing"
          bind:value={s.image.antiAliasing}
          onchange={custom}
          options={[
            { value: 'none', label: 'Nessuno' },
            { value: 'fxaa', label: 'FXAA' },
            { value: 'smaa', label: 'SMAA', recommended: true },
          ]}
        />
      </SettingRow>
    {/if}
    <SettingRow label="Filtro anisotropico" description="Texture del tracciato più definite in lontananza." cost={1}>
      <Segmented
        label="Filtro anisotropico"
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

  <h2>Fluidità</h2>
  <div class="sheet">
    <SettingRow
      label="Frame generation"
      description="Inserisce fotogrammi intermedi: il gioco resta a 60, lo schermo ne mostra di più. Aggiunge un po' di latenza."
      tag={s.smoothness.frameGen === 'off' ? null : 'Latenza +'}
      tone="giallo"
      stacked
    >
      <Segmented
        label="Frame generation"
        bind:value={s.smoothness.frameGen}
        onchange={custom}
        options={native
          ? [
              { value: 'off', label: 'Spenta' },
              { value: 'dlss', label: 'DLSS (2x)', disabled: !nvidiaRtx },
              { value: 'fsr3', label: 'FSR 3 (2x)', recommended: !nvidiaRtx },
              { value: 'builtin', label: 'Integrata' },
            ]
          : [
              { value: 'off', label: 'Spenta' },
              { value: 'lsfg', label: 'Lossless Scaling', disabled: !lossless },
            ]}
      />
    </SettingRow>
    {#if !native && !lossless}
      <p class="aside">
        Per la frame generation in Compat serve Lossless Scaling (Steam): Deluxe8 usa il tuo <code>Lossless.dll</code>
        senza copiarlo altrove.
      </p>
    {/if}
    {#if s.smoothness.frameGen === 'lsfg' || (native && s.smoothness.frameGen === 'builtin')}
      <SettingRow label="Moltiplicatore" description="Quanti fotogrammi a schermo per ogni fotogramma del gioco.">
        <Segmented
          label="Moltiplicatore"
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
      <SettingRow label="Scala del flusso" description="Precisione della stima del movimento. 0 lascia decidere a Deluxe8.">
        <Slider label="Scala del flusso" bind:value={s.smoothness.flowScale} min={0} max={100} step={5} format={(v) => (v === 0 ? 'Auto' : `${v}%`)} onchange={custom} />
      </SettingRow>
    {/if}
    {#if nvidiaRtx}
      <p class="aside">
        Alternativa senza configurazione: NVIDIA Smooth Motion (RTX 40/50) genera fotogrammi a livello di driver
        anche per giochi Vulkan. Si attiva dall'app NVIDIA, per programma.
      </p>
    {/if}
    <SettingRow label="Sincronizzazione verticale" description="FIFO elimina lo strappo; Mailbox riduce la latenza se lo schermo va oltre 60 Hz.">
      <Segmented
        label="V-Sync"
        bind:value={s.display.vsync}
        onchange={custom}
        options={[
          { value: 'fifo', label: 'FIFO', recommended: true },
          { value: 'mailbox', label: 'Mailbox' },
          { value: 'fifo_relaxed', label: 'FIFO rilassato' },
          { value: 'immediate', label: 'Disattivata' },
        ]}
      />
    </SettingRow>
    <SettingRow label="Indicatore prestazioni" description="FPS e tempi del fotogramma in gioco. Si cambia anche dal menu L3 + R3.">
      <Segmented
        label="Indicatore prestazioni"
        bind:value={s.smoothness.hud}
        options={[
          { value: 'off', label: 'Spento' },
          { value: 'compact', label: 'Essenziale' },
          { value: 'full', label: 'Completo' },
        ]}
      />
    </SettingRow>
  </div>

  {#if native}
    <h2>HDR</h2>
    <div class="sheet">
      <SettingRow label="HDR10" description="Richiede lo schermo in modalità HDR. La mappatura dei toni è calcolata dalla scena HDR del gioco.">
        <Toggle label="HDR10" bind:checked={s.image.hdr} onchange={custom} />
      </SettingRow>
      {#if s.image.hdr}
        <SettingRow label="Bianco carta" description="Luminosità di HUD e menu, in nit." stacked>
          <Segmented
            label="Bianco carta"
            bind:value={s.image.hdrPaperWhite}
            onchange={custom}
            options={hdrPaperWhites.map((n) => ({ value: n, label: `${n} nit`, recommended: n === 203 }))}
          />
        </SettingRow>
        <SettingRow label="Picco" description="Luminosità massima del tuo schermo, in nit." stacked>
          <Segmented
            label="Picco"
            bind:value={s.image.hdrPeak}
            onchange={custom}
            options={hdrPeaks.map((n) => ({ value: n, label: `${n} nit` }))}
          />
        </SettingRow>
      {/if}
    </div>
  {/if}

  <h2>Schermo</h2>
  <div class="sheet">
    <SettingRow label="Schermo intero" description="Senza bordi sul monitor principale.">
      <Toggle label="Schermo intero" bind:checked={s.display.fullscreen} />
    </SettingRow>
    {#if !native}
      <SettingRow label="Formato" description="Il gioco è disegnato in 16:9: gli altri formati lo allungano.">
        <Segmented
          label="Formato"
          bind:value={s.display.aspect}
          options={[
            { value: '16:9', label: '16:9', recommended: true },
            { value: '16:10', label: '16:10' },
            { value: '21:9', label: '21:9' },
            { value: 'stretch', label: 'Adatta' },
          ]}
        />
      </SettingRow>
    {/if}
  </div>

  <h2>Avanzate</h2>
  <div class="sheet">
    {#if !native}
      <SettingRow label="Precisione GPU" description="Precisa è l'impostazione di riferimento; Veloce guadagna qualche fotogramma sui PC deboli con rari difetti grafici." cost={s.advanced.gpuAccuracy === 'high' ? 2 : 1}>
        <Segmented
          label="Precisione GPU"
          bind:value={s.advanced.gpuAccuracy}
          onchange={custom}
          options={[
            { value: 'high', label: 'Precisa', recommended: true },
            { value: 'normal', label: 'Veloce' },
          ]}
        />
      </SettingRow>
      <SettingRow label="Shader asincroni" description="Meno scatti la prima volta che compare un effetto, a costo di brevi elementi mancanti.">
        <Toggle label="Shader asincroni" bind:checked={s.advanced.asyncShaders} onchange={custom} />
      </SettingRow>
      <SettingRow label="Decodifica texture ASTC" description="Sulla GPU è più veloce; sulla CPU in modo asincrono è la più fluida al primo caricamento.">
        <Segmented
          label="Decodifica ASTC"
          bind:value={s.advanced.astcDecode}
          onchange={custom}
          options={[
            { value: 'gpu', label: 'GPU', recommended: true },
            { value: 'async', label: 'CPU asincrona' },
            { value: 'cpu', label: 'CPU' },
          ]}
        />
      </SettingRow>
      <SettingRow label="Uso della VRAM" description="Aggressivo tiene più dati in memoria video: meno caricamenti, più VRAM occupata.">
        <Segmented
          label="Uso della VRAM"
          bind:value={s.advanced.vramMode}
          onchange={custom}
          options={[
            { value: 'conservative', label: 'Conservativo' },
            { value: 'aggressive', label: 'Aggressivo', recommended: (gpu?.vramMb ?? 0) >= 8000 },
          ]}
        />
      </SettingRow>
    {/if}
    <SettingRow label="Cache degli shader su disco" description="Conserva gli shader compilati: dalla seconda partita niente scatti.">
      <Toggle label="Cache degli shader" bind:checked={s.advanced.diskShaderCache} />
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
  code {
    font-family: inherit;
    font-stretch: 85%;
    background: var(--pannello-2);
    padding: 0 5px;
    border-radius: 3px;
  }
</style>
