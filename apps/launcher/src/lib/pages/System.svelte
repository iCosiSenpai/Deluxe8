<script lang="ts">
  import { onMount } from 'svelte';
  import { api, on, openUrl } from '../api';
  import { app } from '../state.svelte';
  import Kerb from '../components/Kerb.svelte';
  import type { ComponentDone, ComponentsStatus, PrepareStep } from '../types';

  let launcherLog = $state('');
  let comp = $state<ComponentsStatus | null>(null);
  let installing = $state<'native' | 'dlss' | null>(null);
  let progress = $state<PrepareStep | null>(null);
  const sys = $derived(app.system);
  const nvidia = $derived(sys?.gpus.find((g) => g.vendor === 'nvidia') ?? null);

  async function refresh() {
    comp = await api.componentsStatus().catch(() => null);
    app.system = await api.systemInfo().catch(() => app.system);
  }

  onMount(() => {
    api.readLog('launcher').then((t) => (launcherLog = t)).catch(() => {});
    refresh();
    const offs = [
      on<PrepareStep>('prepare://step', (p) => {
        if (installing) progress = p;
      }),
      on<ComponentDone>('component://done', (d) => {
        installing = null;
        progress = null;
        if (d.ok) app.toast(d.name === 'native' ? 'Motore Nativo installato' : 'NVIDIA DLSS pronto', 'ok');
        else app.toast(d.message ?? 'Installazione non riuscita', 'error', 8000);
        refresh();
      }),
    ];
    return () => offs.forEach((p) => p.then((f) => f()));
  });

  async function install(name: 'native' | 'dlss') {
    installing = name;
    progress = null;
    try {
      await api.installComponent(name);
    } catch (e) {
      installing = null;
      app.toast(String(e), 'error');
    }
  }

  function openFolder(which: 'logs' | 'data') {
    api.openFolder(which).catch((e) => app.toast(`Cartella non aperta: ${e}`, 'error'));
  }
  function redoSetup() {
    app.settings.setupDone = false;
  }
</script>

<section class="page">
  <h1 class="display">Sistema</h1>

  <div class="cols">
    <div class="sheet">
      <h2>Questo PC</h2>
      {#if sys}
        <dl>
          <dt>Sistema</dt><dd>{sys.os}</dd>
          <dt>Processore</dt><dd>{sys.cpu}, {sys.cores} thread</dd>
          <dt>Memoria</dt><dd class="data">{sys.ramGb} GB</dd>
          {#each sys.gpus as g (g.name)}
            <dt>Scheda video</dt><dd>{g.name}{g.driver ? `, driver ${g.driver}` : ''}</dd>
          {/each}
          {#each sys.displays as d, i (i)}
            <dt>Schermo {i + 1}</dt><dd class="data">{d.width}×{d.height} a {d.refreshHz} Hz</dd>
          {/each}
          <dt>Lossless Scaling</dt><dd>{sys.losslessDll ? 'Trovato' : 'Non installato'}</dd>
        </dl>
      {/if}
    </div>
    <div class="sheet">
      <h2>Motori</h2>
      {#if sys}
        <dl>
          <dt>Compat</dt><dd class:ok={sys.compatEngine}>{sys.compatEngine ? 'Pronto' : 'Non trovato'}</dd>
          <dt>Nativo</dt><dd class:ok={sys.nativeEngine}>{sys.nativeEngine ? 'Pronto' : 'Da scaricare'}</dd>
          <dt>Cartella dati</dt><dd class="data small">{sys.dataDir}</dd>
        </dl>
      {/if}
      <div class="actions">
        <button class="ghost" onclick={() => openFolder('data')}>Apri cartella dati</button>
        <button class="ghost" onclick={() => openFolder('logs')}>Apri i log</button>
        <button class="ghost" onclick={redoSetup}>Rifai la preparazione</button>
      </div>
    </div>
  </div>

  <h2 class="spaced">Componenti da scaricare</h2>
  <div class="cols">
    <div class="sheet comp" class:ready={!!comp?.nativeEngine}>
      <h3>Motore Nativo</h3>
      <p>
        Racing 8 DX Recomp v{comp?.nativeVersion ?? '…'}, dalla sua pagina delle release su GitHub
        ({comp?.nativeDownloadMb ?? '…'} MB). Il gioco viene poi ricompilato sul tuo PC dalla preparazione.
      </p>
      {#if comp?.nativeEngine}
        <p class="state ok">Installato</p>
        <p class="data small">{comp.nativeEngine}</p>
      {:else if comp && !comp.nativeDownloadable}
        <p class="state warn">Su questo sistema si installa a mano (vedi la documentazione).</p>
      {:else}
        <button class="ghost" disabled={!!installing || !comp} onclick={() => install('native')}>
          Scarica da GitHub
        </button>
      {/if}
      {#if installing === 'native' && progress}
        <Kerb value={progress.progress ?? 0} label={progress.label} />
        <p class="small">{progress.detail ?? ''}</p>
      {/if}
    </div>
    <div class="sheet comp" class:ready={!!comp?.dlssInstalled}>
      <h3>NVIDIA DLSS</h3>
      <p>
        DLSS SDK {comp?.dlssVersion ?? '…'} da github.com/NVIDIA/DLSS ({comp?.dlssDownloadMb ?? '…'} MB), compilato sul tuo PC
        per il motore Nativo. I file di NVIDIA restano qui e Deluxe8 non li condivide.
      </p>
      {#if comp?.dlssInstalled}
        <p class="state ok">Pronto</p>
      {:else if !nvidia}
        <p class="state">Serve una scheda NVIDIA RTX.</p>
      {:else if comp && !comp.dlssBuildable}
        <p class="state warn">Non disponibile su questo sistema dall'app.</p>
      {:else}
        <p class="small">
          Scaricandolo accetti la
          <button class="link" onclick={() => comp && openUrl(comp.dlssLicenseUrl)}>licenza del DLSS SDK di NVIDIA</button>.
        </p>
        <button class="ghost" disabled={!!installing || !comp?.nativeEngine} onclick={() => install('dlss')}>
          Scarica e prepara
        </button>
        {#if !comp?.nativeEngine}<p class="small">Prima serve il motore Nativo.</p>{/if}
      {/if}
      {#if installing === 'dlss' && progress}
        <Kerb value={progress.progress ?? 0} label={progress.label} />
        <p class="small">{progress.detail ?? ''}</p>
      {/if}
    </div>
  </div>

  <h2 class="spaced">Ultimi messaggi del motore</h2>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <pre class="log" tabindex="0">{app.engineLog.slice(-120).join('\n') || 'Nessun messaggio: avvia il gioco per vederli qui.'}</pre>

  <h2 class="spaced">Launcher</h2>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <pre class="log" tabindex="0">{launcherLog || '—'}</pre>

  <h2 class="spaced">Note legali</h2>
  <div class="sheet legal">
    <p>
      Deluxe8 è software libero (GPL-3.0-or-later), non è affiliato a Nintendo e non contiene codice, chiavi,
      firmware o contenuti di Nintendo. Funziona solo con file che possiedi. Il motore Compat deriva da Eden
      (GPL-3.0); il motore Nativo deriva da Racing 8 DX Recomp (GPL-3.0). La frame generation LSFG usa il
      <code>Lossless.dll</code> della tua copia di Lossless Scaling. DLSS usa il DLSS SDK che scarichi da NVIDIA sul tuo PC
      (licenza NVIDIA) e le librerie del tuo driver: non viene mai ridistribuito.
    </p>
  </div>
</section>

<style>
  .page {
    padding: var(--s6) var(--s7) var(--s7);
    max-width: 1120px;
  }
  h1 {
    font-size: var(--fs-3xl);
    margin-bottom: var(--s5);
  }
  h2 {
    font-size: var(--fs-lg);
    font-weight: 760;
    margin-bottom: var(--s3);
  }
  .spaced {
    margin-top: var(--s6);
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--s4);
  }
  .sheet {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: var(--s5);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px var(--s4);
    margin: 0;
    font-size: var(--fs-sm);
  }
  dt {
    color: var(--gesso-3);
  }
  dd {
    margin: 0;
    word-break: break-word;
  }
  dd.ok {
    color: var(--verde);
  }
  .small {
    font-size: var(--fs-xs);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2);
    margin-top: var(--s4);
  }
  .ghost {
    border-radius: var(--r-ctl);
    padding: 8px 14px;
    font-weight: 600;
    background: transparent;
    border: 1px solid var(--linea-forte);
  }
  .log {
    background: #0d1118;
    border: 1px solid var(--linea);
    border-radius: var(--r-panel);
    padding: var(--s4);
    max-height: 260px;
    overflow: auto;
    font-family: 'Cascadia Mono', 'Consolas', monospace;
    font-size: 12px;
    line-height: 1.5;
    color: var(--gesso-2);
    user-select: text;
    white-space: pre-wrap;
  }
  .comp {
    display: grid;
    align-content: start;
    gap: var(--s2);
    border-top: 4px solid var(--linea);
    min-width: 0;
  }
  .comp .data {
    word-break: break-all;
  }
  .comp.ready {
    border-top-color: var(--verde);
  }
  .comp h3 {
    font-size: var(--fs-md);
    font-weight: 750;
  }
  .comp p {
    margin: 0;
    color: var(--gesso-2);
    font-size: var(--fs-sm);
  }
  .comp .state.ok {
    color: var(--verde);
  }
  .comp .state.warn {
    color: var(--giallo);
  }
  .comp .ghost {
    justify-self: start;
  }
  .ghost:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .link {
    background: none;
    border: 0;
    padding: 0;
    color: var(--gesso);
    text-decoration: underline;
    font: inherit;
    cursor: pointer;
  }
  .legal p {
    color: var(--gesso-2);
    font-size: var(--fs-sm);
    max-width: 90ch;
  }
  code {
    font-family: inherit;
    background: var(--pannello-2);
    padding: 0 5px;
    border-radius: 3px;
  }
  @media (max-width: 900px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
</style>
