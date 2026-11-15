<script lang="ts">
  import { onMount } from 'svelte';
  import { api, demo, on, openUrl, pickPath } from '../api';
  import { locale, resolveLocale, tr, type LocaleSetting } from '../i18n.svelte';
  import { app } from '../state.svelte';
  import { presets } from '../defaults';
  import Kerb from '../components/Kerb.svelte';
  import Segmented from '../components/Segmented.svelte';
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

  // English source text, shown through tr
  const steps = ['Welcome', 'Your game', 'Keys and firmware', 'Engine', 'Preparation'] as const;
  let step = $state(Math.min(3, Number(demo?.get('step') ?? 0) || 0));

  let candidates = $state<Candidates | null>(null);
  let scan = $state<GameScan | null>(null);
  let keys = $state<KeysReport | null>(null);
  let firmware = $state<FirmwareReport | null>(null);
  let busy = $state(false);
  let prep = $state<PrepareStep[]>([]);
  let prepDone = $state<null | { ok: boolean; message?: string }>(null);
  let comp = $state<ComponentsStatus | null>(null);

  const s = app.settings;
  const autoName = $derived(resolveLocale('auto', app.system?.locale) === 'it' ? 'Italiano' : 'English');

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
    }).catch((e) => app.toast(tr('Automatic search failed: {error}', { error: String(e) }), 'warn'));
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

  function setLanguage(v: LocaleSetting) {
    s.ui.language = v;
    app.applyLanguage();
  }

  async function useGameFolder(path: string) {
    busy = true;
    try {
      scan = await api.scanGameFolder(path);
      s.paths.gameFolder = path;
      s.paths.base = scan.base?.path ?? '';
      s.paths.update = scan.update?.path ?? '';
      s.paths.dlc = scan.dlc.map((d) => d.path);
    } catch (e) {
      app.toast(tr('Folder not readable: {error}', { error: String(e) }), 'error');
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
      app.toast(tr('Not a valid keys file: {error}', { error: String(e) }), 'error');
    }
  }
  async function useFirmware(path: string) {
    try {
      firmware = await api.inspectFirmware(path);
      s.paths.firmware = path;
    } catch (e) {
      app.toast(tr('Not a valid firmware: {error}', { error: String(e) }), 'error');
    }
  }

  async function chooseGame() {
    const p = await pickPath({ directory: true, title: tr('Folder with the game, the update and the DLC') });
    if (p) await useGameFolder(p);
  }
  async function chooseKeys() {
    const p = await pickPath({ title: tr('Select prod.keys'), filters: [{ name: tr('Keys'), extensions: ['keys'] }] });
    if (p) await useKeys(p);
  }
  async function chooseFirmwareDir() {
    const p = await pickPath({ directory: true, title: tr('Firmware folder (.nca files)') });
    if (p) await useFirmware(p);
  }
  async function chooseFirmwareZip() {
    const p = await pickPath({ title: tr('Firmware archive'), filters: [{ name: tr('ZIP archive'), extensions: ['zip'] }] });
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
    app.page = s.engine === 'online' ? 'online' : 'home';
  }

  function pkgTone(p: PackageInfo | null | undefined): 'verde' | 'giallo' | 'rosso' {
    if (!p) return 'rosso';
    return p.note ? 'giallo' : 'verde';
  }
  const gb = (n: number) =>
    (n / 1024 ** 3).toLocaleString(locale(), { minimumFractionDigits: 2, maximumFractionDigits: 2 }) + ' GB';
  const masterHex = (n: number | null) => (n === null ? '–' : n.toString(16).padStart(2, '0'));
  const gpu = $derived(app.system?.gpus[0] ?? null);
  const nvidia = $derived(app.system?.gpus.find((g) => g.vendor === 'nvidia') ?? null);
  const nativeAvailable = $derived(!!app.system?.nativeEngine || !!comp?.nativeDownloadable);
  const stateLabel = (p: PrepareStep) =>
    p.state === 'done'
      ? tr('Done')
      : p.state === 'running'
        ? p.progress !== null
          ? `${Math.round(p.progress * 100)}%`
          : tr('Running')
        : p.state === 'skipped'
          ? tr('Not needed')
          : p.state === 'error'
            ? tr('Error')
            : tr('Waiting');
</script>

<div class="wizard">
  <ol class="rail" aria-label={tr('Steps')}>
    {#each steps as label, i (label)}
      <li class:done={i < step} class:current={i === step}>
        <span class="n data">{i + 1}</span>
        <span>{tr(label)}</span>
      </li>
    {/each}
  </ol>

  <section class="panel">
    {#if step === 0}
      <h1 class="display title">{tr("Let's prepare your copy")}</h1>
      <p class="lead">
        {tr('Deluxe8 turns the files of your game into a PC version, with high resolution, upscaling, frame generation and online races. You need three things, all of them yours:')}
      </p>
      <ul class="needs">
        <li><strong>{tr('The game')}</strong> {tr('with update 4.0.1 (and the DLC, if you have it), as NSP or NSZ.')}</li>
        <li><strong>{tr('The keys')}</strong> {tr('of your console (prod.keys, and title.keys if you have it).')}</li>
        <li><strong>{tr('The firmware')}</strong> {tr('of your console, as a folder of .nca files or as a .zip.')}</li>
      </ul>
      <p class="fine">
        {tr("Deluxe8 contains nothing from Nintendo and never downloads games, keys or firmware. Your files stay on your PC; the ones it creates (conversions, caches) live in Deluxe8's data folder.")}
      </p>
      <div class="lang-pick">
        <span class="fine">{tr('Language')}</span>
        <Segmented
          label={tr('Language')}
          value={s.ui.language}
          onchange={(v) => setLanguage(v)}
          options={[
            { value: 'auto', label: tr('Automatic ({name})', { name: autoName }) },
            { value: 'en', label: 'English' },
            { value: 'it', label: 'Italiano' },
          ]}
        />
      </div>
    {:else if step === 1}
      <h1 class="display title">{tr('Your game')}</h1>
      <p class="lead">{tr('Choose the folder with the base game, the update and any DLC.')}</p>
      <div class="pick">
        <button class="primary" onclick={chooseGame} data-autofocus>{tr('Choose folder')}</button>
        {#if candidates?.gameFolders.length}
          {#each candidates.gameFolders.filter((f) => f !== scan?.folder) as f (f)}
            <button class="ghost" onclick={() => useGameFolder(f)}>{tr('Use {path}', { path: f })}</button>
          {/each}
        {/if}
      </div>
      {#if busy}<p class="muted">{tr('Reading the files…')}</p>{/if}
      {#if scan}
        <p class="path data">{scan.folder}</p>
        <div class="pkgs">
          {#each [{ label: tr('Base game'), p: scan.base }, { label: tr('Update'), p: scan.update }] as row (row.label)}
            <div class="pkg {pkgTone(row.p)}">
              <span class="bar" aria-hidden="true"></span>
              <div class="pkg-main">
                <span class="pkg-label">{row.label}</span>
                {#if row.p}
                  <span class="pkg-file">{row.p.fileName}</span>
                  {#if row.p.note}<span class="pkg-note">{row.p.note}</span>{/if}
                {:else}
                  <span class="pkg-note">{tr('Not found in this folder')}</span>
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
      <h1 class="display title">{tr('Keys and firmware')}</h1>
      <p class="lead">{tr("They are needed to read the game's files. They are copied only into Deluxe8's data folder.")}</p>
      <div class="two">
        <div class="card {keys ? (keys.ok ? 'verde' : 'rosso') : ''}">
          <h2>{tr('Keys')}</h2>
          {#if keys}
            <p class="path data">{keys.path}</p>
            <dl>
              <dt>{tr('Entries read')}</dt><dd class="data">{keys.count}</dd>
              <dt>{tr('Header key')}</dt><dd>{keys.hasHeaderKey ? tr('Present') : tr('Missing')}</dd>
              <dt>{tr('Master key revision')}</dt><dd class="data">{masterHex(keys.maxMasterKeyRevision)}</dd>
              <dt>title.keys</dt><dd>{keys.titleKeysCount !== null ? tr('{n} entries', { n: keys.titleKeysCount }) : tr('Not found')}</dd>
            </dl>
            {#if keys.missingRequired.length}
              <p class="warn">{tr('Missing: {names}', { names: keys.missingRequired.join(', ') })}</p>
            {/if}
          {:else}
            <p class="muted">{tr('No file selected.')}</p>
          {/if}
          <button class="primary" onclick={chooseKeys} data-autofocus>{tr('Choose prod.keys')}</button>
        </div>
        <div class="card {firmware ? (firmware.ok ? 'verde' : 'rosso') : ''}">
          <h2>{tr('Firmware')}</h2>
          {#if firmware}
            <p class="path data">{firmware.path}</p>
            <dl>
              <dt>{tr('Version')}</dt><dd class="data">{firmware.versionHint ?? tr('Not stated')}</dd>
              <dt>{tr('NCA files')}</dt><dd class="data">{firmware.ncaCount}</dd>
            </dl>
          {:else}
            <p class="muted">{tr('No firmware selected.')}</p>
          {/if}
          <div class="pick">
            <button class="primary" onclick={chooseFirmwareDir}>{tr('Choose folder')}</button>
            <button class="ghost" onclick={chooseFirmwareZip}>{tr('Choose .zip')}</button>
          </div>
        </div>
      </div>
      {#if keys?.ok && firmware?.versionHint}
        <p class="fine">{tr('The keys must be the same version as the firmware, or newer.')}</p>
      {/if}
    {:else if step === 3}
      <h1 class="display title">{tr('Choose the engine')}</h1>
      <p class="lead">{tr('You can change it any time from the Play screen.')}</p>
      <div class="engines" role="radiogroup" aria-label={tr('Engine')} data-horizontal>
        <button
          class="engine"
          role="radio"
          aria-checked={s.engine === 'native'}
          disabled={!nativeAvailable}
          onclick={() => (s.engine = 'native')}
        >
          <span class="e-name display">{tr('Native')}</span>
          <span class="e-sub">{tr('Static recompilation on your PC')}</span>
          <ul>
            <li>{tr("The game's code translated into an x86-64 program, with no CPU emulation")}</li>
            <li>{tr('FSR 3.1, DLSS and frame generation with real motion vectors')}</li>
            <li>{tr('HDR10, 720p to 4K, scene scale up to 200%')}</li>
            <li>{tr('No online play yet')}</li>
          </ul>
          {#if !app.system?.nativeEngine}
            {#if comp?.nativeDownloadable}
              <span class="e-note">
                {tr('Downloaded from GitHub during the preparation: Racing 8 DX Recomp v{version}, {mb} MB. Then the game is recompiled on your PC (5 to 15 minutes).', { version: comp.nativeVersion, mb: comp.nativeDownloadMb })}
              </span>
            {:else}
              <span class="e-warn">{tr('The Native engine is not available on this PC')}</span>
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
          <span class="e-name display">{tr('Emulator')}</span>
          <span class="e-sub">{tr('Tailored Switch emulation')}</span>
          <ul>
            <li>{tr('The widest compatibility with every mode of the game')}</li>
            <li>{tr('Online rooms, also with players on other emulators')}</li>
            <li>{tr('Resolution up to 8x, FSR 1, SGSR, SMAA')}</li>
            <li>{tr('LSFG frame generation with your Lossless Scaling')}</li>
          </ul>
        </button>
        <button
          class="engine"
          role="radio"
          aria-checked={s.engine === 'online'}
          onclick={() => (s.engine = 'online')}
        >
          <span class="e-name display">{tr('Online')}</span>
          <span class="e-sub">{tr('Ryujinx Nextendo on the Nextendo Network')}</span>
          <ul>
            <li>{tr('Worldwide races, tournaments and friends, like the original service')}</li>
            <li>{tr('Race with players on consoles and on other emulators')}</li>
            <li>{tr('Free Nextendo account, created on nextendo.network')}</li>
            <li>{tr("Graphics set in Ryujinx's own window")}</li>
          </ul>
          <span class="e-note">
            {tr('Downloaded during the preparation: Ryujinx Nextendo v{version}, {mb} MB, from the Nextendo team.', { version: comp?.onlineVersion ?? '…', mb: comp?.onlineDownloadMb ?? 42 })}
          </span>
        </button>
      </div>
      {#if s.engine === 'native' && nvidia && comp?.dlssBuildable}
        <div class="card dlss" class:verde={s.native.dlss || comp.dlssInstalled}>
          <div class="dlss-text">
            <h2>NVIDIA DLSS</h2>
            {#if comp.dlssInstalled}
              <p>{tr('Already set up on this PC (DLSS SDK {version}).', { version: comp.dlssVersion })}</p>
            {:else}
              <p>
                {tr('Downloads the DLSS SDK {version} from github.com/NVIDIA/DLSS ({mb} MB) and builds the plugin for your {gpu}: super resolution, DLAA and frame generation in the Native engine.', { version: comp.dlssVersion, mb: comp.dlssDownloadMb, gpu: nvidia.name })}
              </p>
              <p class="fine">
                {tr("NVIDIA's files stay on your PC and Deluxe8 never shares them. By turning on the download you accept the")}
                <button class="link" onclick={() => openUrl(comp!.dlssLicenseUrl)}>{tr("licence of NVIDIA's DLSS SDK")}</button>.
              </p>
            {/if}
          </div>
          {#if !comp.dlssInstalled}
            <Toggle bind:checked={s.native.dlss} label={tr('Download and build NVIDIA DLSS')} />
          {/if}
        </div>
      {/if}
      {#if gpu}
        <p class="fine">{tr('Graphics card found:')} <strong>{gpu.name}</strong>{gpu.vramMb ? `, ${Math.round(gpu.vramMb / 1024)} GB` : ''}.</p>
      {/if}
    {:else}
      <h1 class="display title">{prepDone?.ok ? tr('All set') : tr('Preparation')}</h1>
      <p class="lead">
        {prepDone?.ok
          ? s.engine === 'online'
            ? tr('The game is ready. Sign in to the Nextendo Network from the Online page, then race.')
            : tr('The game is ready. The first race compiles shaders: the first seconds may stutter.')
          : tr('Deluxe8 is preparing the files. Conversions happen only once.')}
      </p>
      <ul class="prep">
        {#each prep as p (p.id)}
          <li class={p.state}>
            <div class="prep-head">
              <span class="prep-label">{p.label}</span>
              <span class="prep-state">{stateLabel(p)}</span>
            </div>
            {#if p.state === 'running' && p.progress !== null}<Kerb value={p.progress} label={p.label} />{/if}
            {#if p.detail}<p class="prep-detail">{p.detail}</p>{/if}
          </li>
        {/each}
      </ul>
      {#if prepDone && !prepDone.ok}
        <p class="warn">{prepDone.message}</p>
        <button class="primary" onclick={startPrepare}>{tr('Try again')}</button>
      {/if}
    {/if}
  </section>

  <div class="actions">
    {#if step > 0 && step < 4}
      <button class="ghost" onclick={() => (step -= 1)}>{tr('Back')}</button>
    {/if}
    <span class="spacer"></span>
    {#if step < 4}
      <button class="primary next" disabled={!canNext || busy} onclick={next}>
        {step === 3 ? tr('Prepare the game') : tr('Next')}
      </button>
    {:else if prepDone?.ok}
      <button class="primary next" onclick={finish} data-autofocus>{tr('Go to the start line')}</button>
    {/if}
  </div>
</div>

<style>
  .lang-pick {
    display: flex;
    align-items: center;
    gap: var(--s4);
    margin-top: var(--s5);
    flex-wrap: wrap;
  }
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
    grid-template-columns: repeat(3, minmax(0, 1fr));
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
