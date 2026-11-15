<script lang="ts">
  import { onMount } from 'svelte';
  import { api, on, openUrl } from '../api';
  import { locale, resolveLocale, tr, type LocaleSetting } from '../i18n.svelte';
  import { app } from '../state.svelte';
  import Kerb from '../components/Kerb.svelte';
  import Segmented from '../components/Segmented.svelte';
  import type { ComponentDone, ComponentsStatus, PrepareStep, Settings } from '../types';

  type Component = 'native' | 'dlss' | 'online';
  const s = app.settings;
  let launcherLog = $state('');
  let comp = $state<ComponentsStatus | null>(null);
  let installing = $state<Component | null>(null);
  let progress = $state<PrepareStep | null>(null);
  const sys = $derived(app.system);
  const nvidia = $derived(sys?.gpus.find((g) => g.vendor === 'nvidia') ?? null);
  const autoName = $derived(resolveLocale('auto', sys?.locale) === 'it' ? 'Italiano' : 'English');

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
        const done: Record<string, string> = {
          native: tr('Native engine installed'),
          dlss: tr('NVIDIA DLSS ready'),
          online: tr('Ryujinx Nextendo is ready'),
        };
        if (d.ok) app.toast(done[d.name] ?? tr('Installed'), 'ok');
        else app.toast(d.message ?? tr('Installation failed'), 'error', 8000);
        refresh();
      }),
    ];
    return () => offs.forEach((p) => p.then((f) => f()));
  });

  async function install(name: Component) {
    installing = name;
    progress = null;
    try {
      await api.installComponent(name);
    } catch (e) {
      installing = null;
      app.toast(String(e), 'error');
    }
  }

  function setLanguage(v: LocaleSetting) {
    s.ui.language = v;
    app.applyLanguage();
  }

  function openFolder(which: 'logs' | 'data') {
    api.openFolder(which).catch((e) => app.toast(tr('Folder not opened: {error}', { error: String(e) }), 'error'));
  }
  function redoSetup() {
    app.settings.setupDone = false;
  }
</script>

<section class="page">
  <h1 class="display">{tr('System')}</h1>

  <h2>{tr('Language')}</h2>
  <div class="sheet lang" lang={locale()}>
    <div class="lang-row">
      <div>
        <p class="lang-title">{tr('Interface')}</p>
        <p class="small">{tr('Automatic follows the language and country of your system.')}</p>
      </div>
      <Segmented
        label={tr('Interface')}
        value={s.ui.language}
        onchange={(v) => setLanguage(v)}
        options={[
          { value: 'auto', label: tr('Automatic ({name})', { name: autoName }) },
          { value: 'en', label: 'English' },
          { value: 'it', label: 'Italiano' },
        ]}
      />
    </div>
    <div class="lang-row">
      <div>
        <p class="lang-title">{tr('Game')}</p>
        <p class="small">{tr("The language of the game's menus and voices. Automatic follows the interface.")}</p>
      </div>
      <Segmented
        label={tr('Game')}
        bind:value={s.system.language}
        options={[
          { value: 'auto' as Settings['system']['language'], label: tr('Automatic') },
          { value: 'en', label: 'English' },
          { value: 'it', label: 'Italiano' },
          { value: 'fr', label: 'Français' },
          { value: 'de', label: 'Deutsch' },
          { value: 'es', label: 'Español' },
          { value: 'ja', label: '日本語' },
        ]}
      />
    </div>
  </div>

  <div class="cols spaced-top">
    <div class="sheet">
      <h2>{tr('This PC')}</h2>
      {#if sys}
        <dl>
          <dt>{tr('Operating system')}</dt><dd>{sys.os}</dd>
          <dt>{tr('Processor')}</dt><dd>{tr('{cpu}, {n} threads', { cpu: sys.cpu, n: sys.cores })}</dd>
          <dt>{tr('Memory')}</dt><dd class="data">{sys.ramGb} GB</dd>
          {#each sys.gpus as g (g.name)}
            <dt>{tr('Graphics card')}</dt><dd>{g.name}{g.driver ? `, driver ${g.driver}` : ''}</dd>
          {/each}
          {#each sys.displays as d, i (i)}
            <dt>{tr('Screen {n}', { n: i + 1 })}</dt><dd class="data">{tr('{w}×{h} at {hz} Hz', { w: d.width, h: d.height, hz: d.refreshHz })}</dd>
          {/each}
          <dt>Lossless Scaling</dt><dd>{sys.losslessDll ? tr('Found') : tr('Not installed')}</dd>
        </dl>
      {/if}
    </div>
    <div class="sheet">
      <h2>{tr('Engines')}</h2>
      {#if sys}
        <dl>
          <dt>{tr('Emulator')}</dt><dd class:ok={sys.compatEngine}>{sys.compatEngine ? tr('Ready') : tr('Not found')}</dd>
          <dt>{tr('Native')}</dt><dd class:ok={sys.nativeEngine}>{sys.nativeEngine ? tr('Ready') : tr('To download')}</dd>
          <dt>{tr('Online')}</dt><dd class:ok={sys.onlineEngine}>{sys.onlineEngine ? tr('Ready') : tr('To download')}</dd>
          <dt>{tr('Data folder')}</dt><dd class="data small">{sys.dataDir}</dd>
        </dl>
      {/if}
      <div class="actions">
        <button class="ghost" onclick={() => openFolder('data')}>{tr('Open the data folder')}</button>
        <button class="ghost" onclick={() => openFolder('logs')}>{tr('Open the logs')}</button>
        <button class="ghost" onclick={redoSetup}>{tr('Prepare the game again')}</button>
      </div>
    </div>
  </div>

  <h2 class="spaced">{tr('Components to download')}</h2>
  <div class="cols three">
    <div class="sheet comp" class:ready={!!comp?.nativeEngine}>
      <h3>{tr('Native engine')}</h3>
      <p>
        {tr('Racing 8 DX Recomp v{version}, from its GitHub releases page ({mb} MB). The preparation then recompiles the game on your PC.', { version: comp?.nativeVersion ?? '…', mb: comp?.nativeDownloadMb ?? '…' })}
      </p>
      {#if comp?.nativeEngine}
        <p class="state ok">{tr('Installed')}</p>
        <p class="data small">{comp.nativeEngine}</p>
      {:else if comp && !comp.nativeDownloadable}
        <p class="state warn">{tr('On this system it is installed by hand (see the documentation).')}</p>
      {:else}
        <button class="ghost" disabled={!!installing || !comp} onclick={() => install('native')}>{tr('Download from GitHub')}</button>
      {/if}
      {#if installing === 'native' && progress}
        <Kerb value={progress.progress ?? 0} label={progress.label} />
        <p class="small">{progress.detail ?? ''}</p>
      {/if}
    </div>
    <div class="sheet comp" class:ready={!!comp?.dlssInstalled}>
      <h3>NVIDIA DLSS</h3>
      <p>
        {tr("DLSS SDK {version} from github.com/NVIDIA/DLSS ({mb} MB), built on your PC for the Native engine. NVIDIA's files stay here and Deluxe8 never shares them.", { version: comp?.dlssVersion ?? '…', mb: comp?.dlssDownloadMb ?? '…' })}
      </p>
      {#if comp?.dlssInstalled}
        <p class="state ok">{tr('Ready')}</p>
      {:else if !nvidia}
        <p class="state">{tr('Needs an NVIDIA RTX card.')}</p>
      {:else if comp && !comp.dlssBuildable}
        <p class="state warn">{tr('Not available from the app on this system.')}</p>
      {:else}
        <p class="small">
          {tr('By downloading it you accept the')}
          <button class="link" onclick={() => comp && openUrl(comp.dlssLicenseUrl)}>{tr("licence of NVIDIA's DLSS SDK")}</button>.
        </p>
        <button class="ghost" disabled={!!installing || !comp?.nativeEngine} onclick={() => install('dlss')}>{tr('Download and build')}</button>
        {#if !comp?.nativeEngine}<p class="small">{tr('The Native engine comes first.')}</p>{/if}
      {/if}
      {#if installing === 'dlss' && progress}
        <Kerb value={progress.progress ?? 0} label={progress.label} />
        <p class="small">{progress.detail ?? ''}</p>
      {/if}
    </div>
    <div class="sheet comp" class:ready={!!comp?.onlineEngine}>
      <h3>Ryujinx Nextendo</h3>
      <p>
        {tr('Version {version} from the Nextendo team ({mb} MB): the emulator that plays online on the Nextendo Network.', { version: comp?.onlineVersion ?? '…', mb: comp?.onlineDownloadMb ?? '…' })}
      </p>
      {#if comp?.onlineEngine}
        <p class="state ok">{comp.nextendoSignedIn ? tr('Installed, Nextendo account linked') : tr('Installed')}</p>
      {:else}
        <button class="ghost" disabled={!!installing || !comp} onclick={() => install('online')}>{tr('Download')}</button>
      {/if}
      {#if installing === 'online' && progress}
        <Kerb value={progress.progress ?? 0} label={progress.label} />
        <p class="small">{progress.detail ?? ''}</p>
      {/if}
    </div>
  </div>

  <h2 class="spaced">{tr('Latest engine messages')}</h2>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <pre class="log" tabindex="0">{app.engineLog.slice(-120).join('\n') || tr('No messages yet: start the game to see them here.')}</pre>

  <h2 class="spaced">{tr('Launcher')}</h2>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <pre class="log" tabindex="0">{launcherLog || '—'}</pre>

  <h2 class="spaced">{tr('Legal notes')}</h2>
  <div class="sheet legal">
    <p>
      {tr("Deluxe8 is free software (GPL-3.0-or-later). It is not affiliated with Nintendo and contains no Nintendo code, keys, firmware or content: it only works with files you own. The Emulator engine is based on Eden (GPL-3.0), the Native engine on Racing 8 DX Recomp (GPL-3.0); online play on the Nextendo Network uses Ryujinx Nextendo (MIT), downloaded from the Nextendo team. LSFG frame generation uses the Lossless.dll of your own Lossless Scaling. DLSS uses the DLSS SDK you download from NVIDIA to your PC (NVIDIA licence) and your driver's libraries: it is never redistributed.")}
    </p>
    <p>{tr('Nintendo Switch and Mario Kart are trademarks of Nintendo. NVIDIA and DLSS are trademarks of NVIDIA; AMD and FidelityFX of AMD.')}</p>
  </div>
</section>

<style>
  .lang {
    padding: var(--s3) var(--s5);
  }
  .lang-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s5);
    padding: var(--s3) 0;
    flex-wrap: wrap;
  }
  .lang-row + .lang-row {
    border-top: 1px solid var(--linea);
  }
  .lang-title {
    font-weight: 650;
    margin: 0;
  }
  .spaced-top {
    margin-top: var(--s5);
  }
  .cols.three {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  @media (max-width: 1180px) {
    .cols.three {
      grid-template-columns: 1fr;
    }
  }
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
  @media (max-width: 900px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
</style>
