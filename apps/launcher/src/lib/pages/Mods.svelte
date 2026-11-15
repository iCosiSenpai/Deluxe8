<script lang="ts">
  import { tr } from '../i18n.svelte';
  import { onMount } from 'svelte';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import Toggle from '../components/Toggle.svelte';
  import type { ModInfo } from '../types';

  let mods = $state<ModInfo[]>([]);
  let folder = $state('');

  async function load() {
    try {
      [mods, folder] = await Promise.all([api.listMods(), api.modsFolder()]);
    } catch (e) {
      app.toast(tr('Mods could not be read: {error}', { error: String(e) }), 'error');
    }
  }
  onMount(load);

  async function toggle(m: ModInfo, enabled: boolean) {
    try {
      await api.setModEnabled(m.name, enabled);
    } catch (e) {
      m.enabled = !enabled;
      app.toast(tr('Could not change the mod: {error}', { error: String(e) }), 'error');
    }
  }
  const mb = (n: number) => (n >= 1e9 ? `${(n / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(n / 1e6))} MB`);
</script>

<section class="page">
  <h1 class="display">{tr('Mods')}</h1>
  <p class="lead">
    {tr('Each mod is a folder with romfs, exefs or cheats inside. Put it in the mods folder and turn it on here. Mods load in the Emulator engine; mods made for 3.0.x can freeze 4.0.1 on the first load.')}
  </p>
  <div class="bar">
    <span class="path data">{folder}</span>
    <button class="ghost" onclick={() => api.openFolder('mods').catch((e) => app.toast(tr('Folder not opened: {error}', { error: String(e) }), 'error'))}>{tr('Open the folder')}</button>
    <button class="ghost" onclick={load}>{tr('Refresh')}</button>
  </div>
  <div class="sheet">
    {#if !mods.length}
      <p class="empty">{tr('No mods installed.')}</p>
    {/if}
    {#each mods as m (m.name)}
      <div class="mod">
        <div>
          <p class="name">{m.name}</p>
          <p class="sub">{m.kinds.join(', ')}, {mb(m.size)}</p>
        </div>
        <Toggle label={m.name} bind:checked={m.enabled} onchange={(v) => toggle(m, v)} />
      </div>
    {/each}
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
    max-width: 74ch;
    margin-bottom: var(--s5);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--s3);
    margin-bottom: var(--s3);
  }
  .path {
    flex: 1;
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sheet {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: 0 var(--s5);
  }
  .mod {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--s4) 0;
    border-bottom: 1px solid var(--linea);
  }
  .mod:last-child {
    border-bottom: 0;
  }
  .name {
    font-weight: 650;
  }
  .sub {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
  }
  .empty {
    padding: var(--s5) 0;
    color: var(--gesso-2);
  }
  .ghost {
    border-radius: var(--r-ctl);
    padding: 8px 14px;
    font-weight: 600;
    background: transparent;
    border: 1px solid var(--linea-forte);
  }
</style>
