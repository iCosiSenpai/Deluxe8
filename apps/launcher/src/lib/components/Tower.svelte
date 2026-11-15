<script lang="ts">
  import { app, pages, type PageId } from '../state.svelte';

  let { badges = {} }: { badges?: Partial<Record<PageId, 'viola' | 'verde' | 'giallo' | 'rosso'>> } =
    $props();

  const engineLabel = $derived(app.settings.engine === 'native' ? 'Nativo' : 'Compat');
</script>

<nav class="tower" aria-label="Sezioni">
  <ol>
    {#each pages as p (p.id)}
      <li>
        <button
          class="row"
          class:active={app.page === p.id}
          aria-current={app.page === p.id ? 'page' : undefined}
          onclick={() => (app.page = p.id)}
        >
          <span class="flag {badges[p.id] ?? ''}" aria-hidden="true"></span>
          <span class="label">{p.label}</span>
          <span class="hint">{p.hint}</span>
        </button>
      </li>
    {/each}
  </ol>

  <div class="status">
    <div class="line">
      <span class="quiet">Motore</span>
      <span class="val">{engineLabel}</span>
    </div>
    <div class="line">
      <span class="quiet">Gioco</span>
      <span class="val data">4.0.1</span>
    </div>
    {#if app.launch.running}
      <div class="line live">
        <span class="dot" aria-hidden="true"></span>
        <span class="val data">{app.launch.fps?.toFixed(0) ?? '--'} fps</span>
      </div>
    {/if}
  </div>
</nav>

<style>
  .tower {
    width: var(--tower-w);
    flex: none;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    background: var(--asfalto-2);
    border-right: 1px solid var(--linea);
    padding: var(--s4) 0;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    position: relative;
    width: 100%;
    display: grid;
    grid-template-columns: 6px 1fr;
    grid-template-rows: auto auto;
    column-gap: var(--s3);
    align-items: center;
    text-align: left;
    padding: 10px var(--s4) 10px 0;
    background: transparent;
    border: 0;
    border-radius: 0;
  }
  .row:hover {
    background: var(--pannello);
  }
  .row.active {
    background: var(--pannello-2);
  }
  .row:focus-visible {
    box-shadow: inset 0 0 0 3px var(--gesso);
  }
  .flag {
    grid-row: 1 / span 2;
    align-self: stretch;
    width: 6px;
    background: transparent;
  }
  .row.active .flag {
    background: var(--gesso);
  }
  .flag.viola { background: var(--viola); }
  .flag.verde { background: var(--verde); }
  .flag.giallo { background: var(--giallo); }
  .flag.rosso { background: var(--rosso); }
  .label {
    font-size: var(--fs-md);
    font-weight: 620;
    letter-spacing: 0.005em;
  }
  .row.active .label {
    font-style: italic;
    font-weight: 820;
    font-stretch: 112%;
  }
  .hint {
    grid-column: 2;
    font-size: var(--fs-xs);
    color: var(--gesso-3);
    line-height: 1.3;
  }
  .status {
    margin: 0 var(--s4);
    padding-top: var(--s4);
    border-top: 1px solid var(--linea);
    display: grid;
    gap: 6px;
    font-size: var(--fs-sm);
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: var(--s2);
  }
  .val {
    color: var(--gesso);
    font-weight: 600;
  }
  .live {
    justify-content: flex-start;
    align-items: center;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--verde);
    box-shadow: 0 0 0 3px var(--verde-velo);
  }
</style>
