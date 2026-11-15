<script lang="ts">
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
      app.toast(`Mod non leggibili: ${e}`, 'error');
    }
  }
  onMount(load);

  async function toggle(m: ModInfo, enabled: boolean) {
    try {
      await api.setModEnabled(m.name, enabled);
    } catch (e) {
      m.enabled = !enabled;
      app.toast(`Non riesco a cambiare la mod: ${e}`, 'error');
    }
  }
  const mb = (n: number) => (n >= 1e9 ? `${(n / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(n / 1e6))} MB`);
</script>

<section class="page">
  <h1 class="display">Mod</h1>
  <p class="lead">
    Ogni mod è una cartella con dentro <code>romfs</code>, <code>exefs</code> o <code>cheats</code>. Mettila nella
    cartella delle mod e attivala qui. Le mod fatte per la 3.0.x possono bloccare la 4.0.1 al primo caricamento.
  </p>
  <div class="bar">
    <span class="path data">{folder}</span>
    <button class="ghost" onclick={() => api.openFolder('mods').catch((e) => app.toast(`Cartella non aperta: ${e}`, 'error'))}>Apri la cartella</button>
    <button class="ghost" onclick={load}>Aggiorna</button>
  </div>
  <div class="sheet">
    {#if !mods.length}
      <p class="empty">Nessuna mod installata.</p>
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
  code {
    font-family: inherit;
    font-stretch: 85%;
    background: var(--pannello-2);
    padding: 0 5px;
    border-radius: 3px;
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
