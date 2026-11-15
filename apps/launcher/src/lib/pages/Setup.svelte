<script lang="ts">
  import { onMount } from 'svelte';
  import { api, on, openUrl, pickPath } from '../api';
  import { app } from '../state.svelte';
  import { presets } from '../defaults';
  import Kerb from '../components/Kerb.svelte';
  import Toggle from '../components/Toggle.svelte';
  import type {
    Candidates,
    ComponentsStatus,
    FirmwareReport,
    GameScan,
    KeysReport,
    PackageInfo,
    PrepareStep,
  } from '../types';

  const steps = ['Benvenuto', 'Il tuo gioco', 'Chiavi e firmware', 'Motore', 'Preparazione'] as const;
  let step = $state(0);

  let candidates = $state<Candidates | null>(null);
  let scan = $state<GameScan | null>(null);
  let keys = $state<KeysReport | null>(null);
  let firmware = $state<FirmwareReport | null>(null);
  let busy = $state(false);
  let prep = $state<PrepareStep[]>([]);
  let prepDone = $state<null | { ok: boolean; message?: string }>(null);
  let comp = $state<ComponentsStatus | null>(null);

  const s = app.settings;

  onMount(() => {
    api.componentsStatus().then((c) => (comp = c)).catch(() => {});
    api.findCandidates().then(async (c) => {
      candidates = c;
      if (!s.paths.gameFolder && c.gameFolders[0]) await useGameFolder(c.gameFolders[0]);
      else if (s.paths.gameFolder) await useGameFolder(s.paths.gameFolder);
      if (!s.paths.prodKeys && c.keys[0]) await useKeys(c.keys[0]);
      else if (s.paths.prodKeys) await useKeys(s.paths.prodKeys);
      if (!s.paths.firmware && c.firmware[0]) await useFirmware(c.firmware[0]);
      else if (s.paths.firmware) await useFirmware(s.paths.firmware);
    }).catch((e) => app.toast(`Ricerca automatica non riuscita: ${e}`, 'warn'));
    const offs = [
      on<PrepareStep>('prepare://step', (p) => {
        const i = prep.findIndex((x) => x.id === p.id);
        if (i >= 0) prep[i] = p;
        else prep.push(p);
      }),
      on<{ ok: boolean; message?: string }>('prepare://done', (d) => {
        prepDone = d;
        busy = false;
      }),
    ];
    const back = () => {
      if (step > 0 && !busy) step -= 1;
    };
    window.addEventListener('d8:back', back);
    return () => {
      offs.forEach((p) => p.then((f) => f()));
      window.removeEventListener('d8:back', back);
    };
  });

  async function useGameFolder(path: string) {
    busy = true;
    try {
      scan = await api.scanGameFolder(path);
      s.paths.gameFolder = path;
      s.paths.base = scan.base?.path ?? '';
      s.paths.update = scan.update?.path ?? '';
      s.paths.dlc = scan.dlc.map((d) => d.path);
    } catch (e) {
      app.toast(`Cartella non leggibile: ${e}`, 'error');
    } finally {
      busy = false;
    }
  }
  async function useKeys(path: string) {
    try {
      keys = await api.inspectKeys(path);
      s.paths.prodKeys = path;
      s.paths.titleKeys = keys.titleKeysPath ?? '';
    } catch (e) {
      app.toast(`File chiavi non valido: ${e}`, 'error');
    }
  }
  async function useFirmware(path: string) {
    try {
      firmware = await api.inspectFirmware(path);
      s.paths.firmware = path;
    } catch (e) {
      app.toast(`Firmware non valido: ${e}`, 'error');
    }
  }

  async function chooseGame() {
    const p = await pickPath({ directory: true, title: 'Cartella con il gioco, l\'aggiornamento e i DLC' });
    if (p) await useGameFolder(p);
  }
  async function chooseKeys() {
    const p = await pickPath({ title: 'Seleziona prod.keys', filters: [{ name: 'Chiavi', extensions: ['keys'] }] });
    if (p) await useKeys(p);
  }
  async function chooseFirmwareDir() {
    const p = await pickPath({ directory: true, title: 'Cartella del firmware (file .nca)' });
    if (p) await useFirmware(p);
  }
  async function chooseFirmwareZip() {
    const p = await pickPath({ title: 'Archivio del firmware', filters: [{ name: 'Archivio ZIP', extensions: ['zip'] }] });
    if (p) await useFirmware(p);
  }

  const gameOk = $derived(!!scan?.base && !!scan?.update);
  const keysOk = $derived(!!keys?.ok);
  const fwOk = $derived(!!firmware?.ok);
  const canNext = $derived(
    step === 0 || (step === 1 && gameOk) || (step === 2 && keysOk && fwOk) || step === 3,
  );

  function next() {
    if (!canNext) return;
    if (step === 3) {
      startPrepare();
    }
    step = Math.min(step + 1, steps.length - 1);
  }

  async function startPrepare() {
    prep = [];
    prepDone = null;
    busy = true;
    try {
      await app.saveNow(); // the preparation reads paths and choices from the saved settings
      await api.prepare(s.engine);
    } catch (e) {
      prepDone = { ok: false, message: String(e) };
      busy = false;
    }
  }

  function finish() {
    presets[s.preset === 'personalizzato' ? 'bilanciato' : s.preset].apply(s);
    s.setupDone = true;
    app.page = 'home';
  }

  function pkgTone(p: PackageInfo | null | undefined): 'verde' | 'giallo' | 'rosso' {
    if (!p) return 'rosso';
    return p.note ? 'giallo' : 'verde';
  }
  const gb = (n: number) => (n / 1024 ** 3).toFixed(2).replace('.', ',') + ' GB';
  const masterHex = (n: number | null) => (n === null ? '–' : n.toString(16).padStart(2, '0'));
  const gpu = $derived(app.system?.gpus[0] ?? null);
  const nvidia = $derived(app.system?.gpus.find((g) => g.vendor === 'nvidia') ?? null);
  const nativeAvailable = $derived(!!app.system?.nativeEngine || !!comp?.nativeDownloadable);
</script>

<div class="wizard">
  <ol class="rail" aria-label="Passaggi">
    {#each steps as label, i (label)}
      <li class:done={i < step} class:current={i === step}>
        <span class="n data">{i + 1}</span>
        <span>{label}</span>
      </li>
    {/each}
  </ol>

  <section class="panel">
    {#if step === 0}
      <h1 class="display title">Prepariamo la tua copia</h1>
      <p class="lead">
        Deluxe8 trasforma i file del tuo gioco in una versione per PC, con risoluzione alta,
        upscaling, frame generation e partite online in stanza. Ti servono tre cose, tutte tue:
      </p>
      <ul class="needs">
        <li><strong>Il gioco</strong> con l'aggiornamento 4.0.1 (e i DLC, se li hai), in formato NSP o NSZ.</li>
        <li><strong>Le chiavi</strong> della tua console (<code>prod.keys</code>, e <code>title.keys</code> se l'hai).</li>
        <li><strong>Il firmware</strong> della tua console, come cartella di file .nca oppure come .zip.</li>
      </ul>
      <p class="fine">
        Deluxe8 non contiene nulla di Nintendo e non scarica giochi, chiavi o firmware. I file restano sul tuo
        PC; quelli che crea (conversioni, cache) stanno nella cartella dati di Deluxe8.
      </p>
    {:else if step === 1}
      <h1 class="display title">Il tuo gioco</h1>
      <p class="lead">Scegli la cartella che contiene il gioco base, l'aggiornamento e gli eventuali DLC.</p>
      <div class="pick">
        <button class="primary" onclick={chooseGame} data-autofocus>Scegli cartella</button>
        {#if candidates?.gameFolders.length}
          {#each candidates.gameFolders.filter((f) => f !== scan?.folder) as f (f)}
            <button class="ghost" onclick={() => useGameFolder(f)}>Usa {f}</button>
          {/each}
        {/if}
      </div>
      {#if busy}<p class="muted">Lettura dei file…</p>{/if}
      {#if scan}
        <p class="path data">{scan.folder}</p>
        <div class="pkgs">
          {#each [{ label: 'Gioco base', p: scan.base }, { label: 'Aggiornamento', p: scan.update }] as row (row.label)}
            <div class="pkg {pkgTone(row.p)}">
              <span class="bar" aria-hidden="true"></span>
              <div class="pkg-main">
                <span class="pkg-label">{row.label}</span>
                {#if row.p}
                  <span class="pkg-file">{row.p.fileName}</span>
                  {#if row.p.note}<span class="pkg-note">{row.p.note}</span>{/if}
                {:else}
                  <span class="pkg-note">Non trovato in questa cartella</span>
                {/if}
              </div>
              {#if row.p}
                <div class="pkg-meta">
                  <span class="big data">{row.p.displayVersion ?? `v${row.p.version}`}</span>
                  <span class="quiet data">{gb(row.p.size)}</span>
                </div>
              {/if}
            </div>
          {/each}
          {#each scan.dlc as d (d.path)}
            <div class="pkg verde">
              <span class="bar" aria-hidden="true"></span>
              <div class="pkg-main">
                <span class="pkg-label">DLC</span>
                <span class="pkg-file">{d.fileName}</span>
              </div>
              <div class="pkg-meta"><span class="quiet data">{d.titleId}</span></div>
            </div>
          {/each}
        </div>
        {#each scan.problems as p (p)}<p class="warn">{p}</p>{/each}
      {/if}
    {:else if step === 2}
      <h1 class="display title">Chiavi e firmware</h1>
      <p class="lead">Servono per leggere i file del gioco. Vengono copiati solo nella cartella dati di Deluxe8.</p>
      <div class="two">
        <div class="card {keys ? (keys.ok ? 'verde' : 'rosso') : ''}">
          <h2>Chiavi</h2>
          {#if keys}
            <p class="path data">{keys.path}</p>
            <dl>
              <dt>Voci lette</dt><dd class="data">{keys.count}</dd>
              <dt>Chiave intestazioni</dt><dd>{keys.hasHeaderKey ? 'Presente' : 'Mancante'}</dd>
              <dt>Revisione master key</dt><dd class="data">{masterHex(keys.maxMasterKeyRevision)}</dd>
              <dt>title.keys</dt><dd>{keys.titleKeysCount !== null ? `${keys.titleKeysCount} voci` : 'Non trovato'}</dd>
            </dl>
            {#if keys.missingRequired.length}
              <p class="warn">Mancano: {keys.missingRequired.join(', ')}</p>
            {/if}
          {:else}
            <p class="muted">Nessun file selezionato.</p>
          {/if}
          <button class="primary" onclick={chooseKeys} data-autofocus>Scegli prod.keys</button>
        </div>
        <div class="card {firmware ? (firmware.ok ? 'verde' : 'rosso') : ''}">
          <h2>Firmware</h2>
          {#if firmware}
            <p class="path data">{firmware.path}</p>
            <dl>
              <dt>Versione</dt><dd class="data">{firmware.versionHint ?? 'Non indicata'}</dd>
              <dt>File NCA</dt><dd class="data">{firmware.ncaCount}</dd>
            </dl>
          {:else}
            <p class="muted">Nessun firmware selezionato.</p>
          {/if}
          <div class="pick">
            <button class="primary" onclick={chooseFirmwareDir}>Scegli cartella</button>
            <button class="ghost" onclick={chooseFirmwareZip}>Scegli .zip</button>
          </div>
        </div>
      </div>
      {#if keys?.ok && firmware?.versionHint}
        <p class="fine">Le chiavi devono essere della stessa versione del firmware o più recenti.</p>
      {/if}
    {:else if step === 3}
      <h1 class="display title">Scegli il motore</h1>
      <p class="lead">Puoi cambiarlo quando vuoi dalla schermata Gioca.</p>
      <div class="engines" role="radiogroup" aria-label="Motore" data-horizontal>
        <button
          class="engine"
          role="radio"
          aria-checked={s.engine === 'native'}
          disabled={!nativeAvailable}
          onclick={() => (s.engine = 'native')}
        >
          <span class="e-name display">Nativo</span>
          <span class="e-sub">Ricompilazione statica sul tuo PC</span>
          <ul>
            <li>Il codice del gioco tradotto in programma x86-64, senza emulazione della CPU</li>
            <li>FSR 3.1, DLSS e frame generation con vettori di movimento reali</li>
            <li>HDR10, 720p–4K, render scale fino al 200%</li>
            <li>Online non ancora disponibile</li>
          </ul>
          {#if !app.system?.nativeEngine}
            {#if comp?.nativeDownloadable}
              <span class="e-note">
                Si scarica da GitHub durante la preparazione: Racing 8 DX Recomp v{comp.nativeVersion},
                {comp.nativeDownloadMb} MB. Poi il gioco viene ricompilato sul tuo PC (5–15 minuti).
              </span>
            {:else}
              <span class="e-warn">Motore Nativo non disponibile su questo PC</span>
            {/if}
          {/if}
        </button>
        <button
          class="engine"
          role="radio"
          aria-checked={s.engine === 'compat'}
          onclick={() => (s.engine = 'compat')}
          data-autofocus
        >
          <span class="e-name display">Compat</span>
          <span class="e-sub">Emulazione su misura (base Eden)</span>
          <ul>
            <li>La massima compatibilità con tutte le modalità del gioco</li>
            <li>Stanze online con altri giocatori</li>
            <li>Risoluzione fino a 8x, FSR 1, SGSR, SMAA</li>
            <li>Frame generation LSFG con il tuo Lossless Scaling</li>
          </ul>
        </button>
      </div>
      {#if s.engine === 'native' && nvidia && comp?.dlssBuildable}
        <div class="card dlss" class:verde={s.native.dlss || comp.dlssInstalled}>
          <div class="dlss-text">
            <h2>NVIDIA DLSS</h2>
            {#if comp.dlssInstalled}
              <p>Già pronto su questo PC (DLSS SDK {comp.dlssVersion}).</p>
            {:else}
              <p>
                Scarica il DLSS SDK {comp.dlssVersion} da github.com/NVIDIA/DLSS ({comp.dlssDownloadMb} MB) e prepara il plugin
                per la tua {nvidia.name}: super resolution, DLAA e frame generation nel motore Nativo.
              </p>
              <p class="fine">
                I file di NVIDIA restano sul tuo PC e Deluxe8 non li condivide. Attivando il download accetti la
                <button class="link" onclick={() => openUrl(comp!.dlssLicenseUrl)}>licenza del DLSS SDK di NVIDIA</button>.
              </p>
            {/if}
          </div>
          {#if !comp.dlssInstalled}
            <Toggle bind:checked={s.native.dlss} label="Scarica e prepara NVIDIA DLSS" />
          {/if}
        </div>
      {/if}
      {#if gpu}
        <p class="fine">Scheda video rilevata: <strong>{gpu.name}</strong>{gpu.vramMb ? `, ${Math.round(gpu.vramMb / 1024)} GB` : ''}.</p>
      {/if}
    {:else}
      <h1 class="display title">{prepDone?.ok ? 'Tutto pronto' : 'Preparazione'}</h1>
      <p class="lead">
        {prepDone?.ok
          ? 'Il gioco è pronto. La prima corsa compila gli shader: i primi secondi possono scattare.'
          : 'Deluxe8 sta preparando i file. Le conversioni avvengono una sola volta.'}
      </p>
      <ul class="prep">
        {#each prep as p (p.id)}
          <li class={p.state}>
            <div class="prep-head">
              <span class="prep-label">{p.label}</span>
              <span class="prep-state">
                {p.state === 'done' ? 'Fatto' : p.state === 'running' ? (p.progress !== null ? `${Math.round(p.progress * 100)}%` : 'In corso') : p.state === 'skipped' ? 'Non necessario' : p.state === 'error' ? 'Errore' : 'In attesa'}
              </span>
            </div>
            {#if p.state === 'running' && p.progress !== null}<Kerb value={p.progress} label={p.label} />{/if}
            {#if p.detail}<p class="prep-detail">{p.detail}</p>{/if}
          </li>
        {/each}
      </ul>
      {#if prepDone && !prepDone.ok}
        <p class="warn">{prepDone.message}</p>
        <button class="primary" onclick={startPrepare}>Riprova</button>
      {/if}
    {/if}
  </section>

  <div class="actions">
    {#if step > 0 && step < 4}
      <button class="ghost" onclick={() => (step -= 1)}>Indietro</button>
    {/if}
    <span class="spacer"></span>
    {#if step < 4}
      <button class="primary next" disabled={!canNext || busy} onclick={next}>
        {step === 3 ? 'Prepara il gioco' : 'Avanti'}
      </button>
    {:else if prepDone?.ok}
      <button class="primary next" onclick={finish} data-autofocus>Vai alla partenza</button>
    {/if}
  </div>
</div>

<style>
  .wizard {
    max-width: 1040px;
    margin: 0 auto;
    padding: var(--s6) var(--s6) var(--s5);
    display: grid;
    gap: var(--s5);
  }
  .rail {
    display: flex;
    gap: var(--s5);
    list-style: none;
    margin: 0;
    padding: 0 0 var(--s4);
    border-bottom: 1px solid var(--linea);
    font-size: var(--fs-sm);
    color: var(--gesso-3);
  }
  .rail li {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  .rail .n {
    display: inline-grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    background: var(--pannello);
    border: 1px solid var(--linea);
    font-size: var(--fs-xs);
  }
  .rail li.done {
    color: var(--gesso-2);
  }
  .rail li.done .n {
    background: var(--verde-velo);
    border-color: var(--verde);
    color: var(--verde);
  }
  .rail li.current {
    color: var(--gesso);
    font-weight: 650;
  }
  .rail li.current .n {
    background: var(--gesso);
    color: var(--asfalto);
    border-color: var(--gesso);
  }
  .panel {
    min-height: 420px;
  }
  .title {
    font-size: var(--fs-3xl);
    margin-bottom: var(--s4);
  }
  .lead {
    font-size: var(--fs-lg);
    color: var(--gesso-2);
    max-width: 60ch;
    margin-bottom: var(--s5);
    line-height: 1.45;
  }
  .needs {
    display: grid;
    gap: var(--s3);
    padding-left: 1.1em;
    max-width: 64ch;
    margin: 0 0 var(--s5);
  }
  .needs li::marker {
    color: var(--viola);
  }
  .fine {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    max-width: 70ch;
  }
  code {
    font-family: inherit;
    font-stretch: 85%;
    background: var(--pannello-2);
    padding: 0 5px;
    border-radius: 3px;
  }
  .pick {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s3);
    margin-bottom: var(--s4);
  }
  .path {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    margin-bottom: var(--s3);
    word-break: break-all;
  }
  .pkgs {
    display: grid;
    gap: var(--s2);
  }
  .pkg {
    display: grid;
    grid-template-columns: 5px 1fr auto;
    gap: var(--s4);
    align-items: center;
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: var(--s3) var(--s4) var(--s3) 0;
    overflow: hidden;
  }
  .pkg .bar {
    align-self: stretch;
    background: var(--linea);
  }
  .pkg.verde .bar { background: var(--verde); }
  .pkg.giallo .bar { background: var(--giallo); }
  .pkg.rosso .bar { background: var(--rosso); }
  .pkg-main {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .pkg-label {
    font-weight: 650;
  }
  .pkg-file {
    font-size: var(--fs-sm);
    color: var(--gesso-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pkg-note {
    font-size: var(--fs-sm);
    color: var(--giallo);
  }
  .pkg.rosso .pkg-note {
    color: #ff8a82;
  }
  .pkg-meta {
    display: grid;
    justify-items: end;
  }
  .big {
    font-size: var(--fs-xl);
    font-style: italic;
    font-weight: 820;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--s4);
  }
  .card {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: var(--s5);
    border-top: 4px solid var(--linea);
    display: grid;
    align-content: start;
    gap: var(--s3);
  }
  .card.verde { border-top-color: var(--verde); }
  .card.rosso { border-top-color: var(--rosso); }
  .card h2 {
    font-size: var(--fs-lg);
    font-weight: 750;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px var(--s4);
    margin: 0;
    font-size: var(--fs-sm);
  }
  dt {
    color: var(--gesso-3);
  }
  dd {
    margin: 0;
  }
  .engines {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--s4);
    margin-bottom: var(--s4);
  }
  .engine {
    text-align: left;
    background: var(--pannello);
    border: 2px solid var(--linea);
    border-radius: var(--r-panel);
    padding: var(--s5);
    display: grid;
    gap: var(--s2);
    align-content: start;
  }
  .engine[aria-checked='true'] {
    border-color: var(--gesso);
    background: var(--pannello-2);
  }
  .engine:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .e-name {
    font-size: var(--fs-2xl);
  }
  .e-sub {
    color: var(--gesso-2);
  }
  .engine ul {
    margin: var(--s2) 0 0;
    padding-left: 1.1em;
    display: grid;
    gap: 4px;
    font-size: var(--fs-sm);
    color: var(--gesso-2);
  }
  .e-warn {
    color: var(--giallo);
    font-size: var(--fs-sm);
  }
  .e-note {
    color: var(--gesso-2);
    font-size: var(--fs-sm);
    border-left: 3px solid var(--viola);
    padding-left: var(--s2);
  }
  .dlss {
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: var(--s4);
    margin-bottom: var(--s4);
  }
  .dlss-text {
    display: grid;
    gap: var(--s2);
  }
  .dlss-text p {
    margin: 0;
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
  .prep {
    list-style: none;
    padding: 0;
    margin: 0 0 var(--s4);
    display: grid;
    gap: var(--s3);
    max-width: 720px;
  }
  .prep li {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: var(--s3) var(--s4);
    display: grid;
    gap: var(--s2);
    border-left: 4px solid var(--linea);
  }
  .prep li.running { border-left-color: var(--viola); }
  .prep li.done { border-left-color: var(--verde); }
  .prep li.error { border-left-color: var(--rosso); }
  .prep li.skipped { border-left-color: var(--gesso-3); }
  .prep-head {
    display: flex;
    justify-content: space-between;
  }
  .prep-label {
    font-weight: 600;
  }
  .prep-state {
    color: var(--gesso-2);
    font-size: var(--fs-sm);
  }
  .prep-detail {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
  }
  .warn {
    color: var(--giallo);
    margin: var(--s3) 0;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--s3);
    padding-top: var(--s4);
    border-top: 1px solid var(--linea);
  }
  .spacer {
    flex: 1;
  }
  .primary,
  .ghost {
    border-radius: var(--r-ctl);
    padding: 10px 18px;
    font-weight: 650;
    border: 1px solid transparent;
  }
  .primary {
    background: var(--gesso);
    color: var(--asfalto);
  }
  .primary:disabled {
    background: var(--linea-forte);
    color: var(--gesso-3);
    cursor: not-allowed;
  }
  .ghost {
    background: transparent;
    border-color: var(--linea-forte);
    color: var(--gesso);
  }
  .ghost:hover {
    background: var(--pannello-2);
  }
  .next {
    min-width: 200px;
    font-size: var(--fs-md);
  }
  @media (max-width: 900px) {
    .two,
    .engines {
      grid-template-columns: 1fr;
    }
  }
</style>
