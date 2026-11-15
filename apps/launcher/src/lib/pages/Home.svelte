<script lang="ts">
  import { tr } from '../i18n.svelte';
  import { api } from '../api';
  import { app, engineName } from '../state.svelte';
  import { presets, type PresetId } from '../defaults';
  import { scaleLabel, upscalerLabel, frameGenLabel } from '../labels';
  import PlayButton from '../components/PlayButton.svelte';
  import Segmented from '../components/Segmented.svelte';

  const s = app.settings;
  let starting = $state(false);

  const nativeReady = $derived(!!app.system?.nativeEngine);
  const compatReady = $derived(!!app.system?.compatEngine);
  const onlineReady = $derived(!!app.system?.onlineEngine);
  const engineReady = $derived(s.engine === 'native' ? nativeReady : s.engine === 'online' ? onlineReady : compatReady);
  const reason = $derived(
    engineReady
      ? ''
      : s.engine === 'native'
        ? tr('The Native engine is not on this PC yet: download it from System > Components and prepare the game again, or choose the Emulator.')
        : s.engine === 'online'
          ? tr('Ryujinx Nextendo is not on this PC yet: set it up from the Online page.')
          : tr("The Emulator engine was not found in Deluxe8's folder."),
  );
  const engineNote = $derived(
    s.engine === 'native'
      ? tr('Code recompiled for your processor: FSR 3.1, DLSS and HDR with real motion vectors.')
      : s.engine === 'online'
        ? tr('Ryujinx Nextendo on the Nextendo Network: online races, rooms and friends with players on consoles and other emulators.')
        : tr('Tailored emulation: the widest compatibility, the in-game overlay and online rooms.'),
  );

  async function launch() {
    starting = true;
    try {
      await api.launch(s.engine);
    } catch (e) {
      app.toast(tr('Could not start: {error}', { error: String(e) }), 'error', 9000);
    } finally {
      starting = false;
    }
  }

  function applyPreset(id: Settings_Preset) {
    if (id === 'personalizzato') return;
    presets[id as PresetId].apply(s);
    s.preset = id;
  }
  type Settings_Preset = typeof s.preset;

  const summary = $derived([
    {
      k: tr('Resolution'),
      v: s.engine === 'native'
        ? tr('{height}p, scene at {scale}%', { height: s.display.outputHeight, scale: s.image.renderScale })
        : scaleLabel(s.image.compatScale),
    },
    { k: tr('Upscaling'), v: upscalerLabel(s.image.upscaler) },
    { k: tr('Anti-aliasing'), v: s.image.antiAliasing === 'none' ? tr('aa|None') : s.image.antiAliasing.toUpperCase() },
    { k: tr('Frame generation'), v: frameGenLabel(s.smoothness.frameGen, s.smoothness.multiplier) },
    { k: tr('Screen'), v: s.display.fullscreen ? tr('Full screen') : tr('Window') },
  ]);
</script>

<section class="home">
  <header class="hero">
    <h1 class="display giant">Mario Kart<br />8 Deluxe</h1>
    <ul class="chips" aria-label={tr('Contents')}>
      <li>{tr('Update {version}', { version: '4.0.1' })}</li>
      {#if s.paths.dlc.length}<li>Booster Course Pass</li>{/if}
      <li class="eng">{tr('{engine} engine', { engine: engineName(s.engine) })}</li>
    </ul>
  </header>

  <PlayButton
    running={app.launch.running}
    disabled={!engineReady || starting}
    {reason}
    onlaunch={launch}
    onstop={() => api.stopEngine()}
  />

  <div class="grid">
    <div class="panel">
      <h2>{tr('Engine')}</h2>
      <Segmented
        label={tr('Engine')}
        bind:value={s.engine}
        options={[
          { value: 'native', label: tr('Native'), disabled: !nativeReady },
          { value: 'compat', label: tr('Emulator') },
          { value: 'online', label: tr('Online') },
        ]}
      />
      <p class="note">{engineNote}</p>

      <h2>{tr('Preset')}</h2>
      <Segmented
        label={tr('Graphics preset')}
        value={s.preset}
        onchange={(v) => applyPreset(v)}
        options={[
          { value: 'qualita', label: tr('Quality') },
          { value: 'bilanciato', label: tr('Balanced'), recommended: true },
          { value: 'prestazioni', label: tr('Performance') },
          { value: 'competitivo', label: tr('Competitive') },
          { value: 'personalizzato', label: tr('Custom'), disabled: s.preset !== 'personalizzato' },
        ]}
      />
      <p class="note">
        {s.preset === 'personalizzato'
          ? tr('You changed the settings by hand. Pick a preset to start again from a known base.')
          : tr(presets[s.preset as PresetId].summary)}
      </p>
    </div>

    <div class="panel">
      {#if app.launch.running}
        <h2>{tr('On track')}</h2>
        <div class="live">
          <span class="fps display">{app.launch.fps?.toFixed(0) ?? '--'}</span>
          <span class="unit">fps</span>
        </div>
        <dl class="sheet">
          <dt>{tr('Frame time')}</dt><dd class="data">{app.launch.frametimeMs?.toFixed(1) ?? '--'} ms</dd>
          <dt>{tr('Speed')}</dt><dd class="data">{app.launch.speed?.toFixed(0) ?? '--'}%</dd>
        </dl>
        <p class="note">
          {s.engine === 'online'
            ? tr('Online menus are in the game itself; Ryujinx Nextendo has its own settings window.')
            : tr('Press L3 + R3 (or F1) in game for the quick menu.')}
        </p>
      {:else}
        <h2>{tr('Current setup')}</h2>
        {#if s.engine === 'online'}
          <p class="note">{tr('Graphics for online play are set in Ryujinx Nextendo (Options > Settings > Graphics).')}</p>
          <button class="link" onclick={() => (app.page = 'online')}>{tr('Open the Online page')}</button>
        {:else}
          <dl class="sheet">
            {#each summary as row (row.k)}
              <dt>{row.k}</dt><dd>{row.v}</dd>
            {/each}
          </dl>
          <button class="link" onclick={() => (app.page = 'grafica')}>{tr('Change the graphics')}</button>
        {/if}
      {/if}
    </div>
  </div>
</section>

<style>
  .home {
    padding: var(--s7) var(--s7) var(--s6);
    display: grid;
    gap: var(--s6);
    max-width: 1200px;
  }
  .giant {
    font-size: var(--fs-hero);
    line-height: 0.88;
    margin-bottom: var(--s4);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2);
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .chips li {
    padding: 4px 12px;
    border-radius: 999px;
    border: 1px solid var(--linea-forte);
    font-size: var(--fs-sm);
    color: var(--gesso-2);
  }
  .chips li.eng {
    border-color: var(--gesso-3);
    color: var(--gesso);
  }
  .grid {
    display: grid;
    grid-template-columns: 1.2fr 1fr;
    gap: var(--s4);
  }
  .panel {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: var(--s5);
    display: grid;
    align-content: start;
    gap: var(--s3);
  }
  h2 {
    font-size: var(--fs-md);
    font-weight: 700;
    color: var(--gesso-2);
  }
  h2:not(:first-child) {
    margin-top: var(--s3);
  }
  .note {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    max-width: 52ch;
  }
  .sheet {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px var(--s5);
    margin: 0;
  }
  dt {
    color: var(--gesso-3);
  }
  dd {
    margin: 0;
    font-weight: 600;
  }
  .live {
    display: flex;
    align-items: baseline;
    gap: var(--s2);
  }
  .fps {
    font-size: var(--fs-3xl);
    color: var(--verde);
  }
  .unit {
    color: var(--gesso-2);
  }
  .link {
    justify-self: start;
    background: transparent;
    border: 0;
    padding: 4px 0;
    color: var(--gesso);
    text-decoration: underline;
    text-underline-offset: 4px;
    font-weight: 600;
  }
  @media (max-width: 1100px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
