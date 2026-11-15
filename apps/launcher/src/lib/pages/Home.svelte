<script lang="ts">
  import { api } from '../api';
  import { app } from '../state.svelte';
  import { presets, type PresetId } from '../defaults';
  import { scaleLabel, upscalerLabel, frameGenLabel } from '../labels';
  import PlayButton from '../components/PlayButton.svelte';
  import Segmented from '../components/Segmented.svelte';

  const s = app.settings;
  let starting = $state(false);

  const nativeReady = $derived(!!app.system?.nativeEngine);
  const compatReady = $derived(!!app.system?.compatEngine);
  const engineReady = $derived(s.engine === 'native' ? nativeReady : compatReady);
  const reason = $derived(
    engineReady
      ? ''
      : s.engine === 'native'
        ? 'Il motore Nativo non è ancora su questo PC: scaricalo da Sistema > Componenti e rifai la preparazione, oppure scegli Compat.'
        : 'Il motore Compat non è stato trovato nella cartella di Deluxe8.',
  );

  async function launch() {
    starting = true;
    try {
      await api.launch(s.engine);
    } catch (e) {
      app.toast(`Avvio non riuscito: ${e}`, 'error', 9000);
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
    { k: 'Risoluzione', v: s.engine === 'native' ? `${s.display.outputHeight}p, scena al ${s.image.renderScale}%` : scaleLabel(s.image.compatScale) },
    { k: 'Upscaling', v: upscalerLabel(s.image.upscaler) },
    { k: 'Anti-aliasing', v: s.image.antiAliasing === 'none' ? 'Nessuno' : s.image.antiAliasing.toUpperCase() },
    { k: 'Frame generation', v: frameGenLabel(s.smoothness.frameGen, s.smoothness.multiplier) },
    { k: 'Schermo', v: s.display.fullscreen ? 'Schermo intero' : 'Finestra' },
  ]);
</script>

<section class="home">
  <header class="hero">
    <h1 class="display giant">Mario Kart<br />8 Deluxe</h1>
    <ul class="chips" aria-label="Contenuti">
      <li>Aggiornamento 4.0.1</li>
      {#if s.paths.dlc.length}<li>Booster Course Pass</li>{/if}
      <li class="eng">Motore {s.engine === 'native' ? 'Nativo' : 'Compat'}</li>
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
      <h2>Motore</h2>
      <Segmented
        label="Motore"
        bind:value={s.engine}
        options={[
          { value: 'native', label: 'Nativo', disabled: !nativeReady },
          { value: 'compat', label: 'Compat' },
        ]}
      />
      <p class="note">
        {s.engine === 'native'
          ? 'Codice ricompilato per il tuo processore: FSR 3.1, DLSS e HDR con vettori di movimento reali.'
          : 'Emulazione su misura: massima compatibilità e stanze online.'}
      </p>

      <h2>Preset</h2>
      <Segmented
        label="Preset grafico"
        value={s.preset}
        onchange={(v) => applyPreset(v)}
        options={[
          { value: 'qualita', label: 'Qualità' },
          { value: 'bilanciato', label: 'Bilanciato', recommended: true },
          { value: 'prestazioni', label: 'Prestazioni' },
          { value: 'competitivo', label: 'Competitivo' },
          { value: 'personalizzato', label: 'Personalizzato', disabled: s.preset !== 'personalizzato' },
        ]}
      />
      <p class="note">
        {s.preset === 'personalizzato'
          ? 'Hai modificato le impostazioni a mano. Scegli un preset per ripartire da una base nota.'
          : presets[s.preset as PresetId].summary}
      </p>
    </div>

    <div class="panel">
      {#if app.launch.running}
        <h2>In pista</h2>
        <div class="live">
          <span class="fps display">{app.launch.fps?.toFixed(0) ?? '--'}</span>
          <span class="unit">fps</span>
        </div>
        <dl class="sheet">
          <dt>Tempo per fotogramma</dt><dd class="data">{app.launch.frametimeMs?.toFixed(1) ?? '--'} ms</dd>
          <dt>Velocità</dt><dd class="data">{app.launch.speed?.toFixed(0) ?? '--'}%</dd>
        </dl>
        <p class="note">Premi L3 + R3 (o F1) in gioco per il menu rapido.</p>
      {:else}
        <h2>Assetto attuale</h2>
        <dl class="sheet">
          {#each summary as row (row.k)}
            <dt>{row.k}</dt><dd>{row.v}</dd>
          {/each}
        </dl>
        <button class="link" onclick={() => (app.page = 'grafica')}>Modifica la grafica</button>
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
