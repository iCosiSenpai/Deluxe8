<script lang="ts">
  import { tr } from '../i18n.svelte';
  /** Controller legend. Shows the device actually in use. */
  let { pad = null }: { pad?: string | null } = $props();
  const hints = [
    { key: 'A', label: 'Select' },
    { key: 'B', label: 'Back' },
    { key: 'LB RB', label: 'Sections' },
    { key: 'Start', label: 'Play' },
  ];
  const shortName = $derived(pad ? pad.replace(/\(.*?\)/g, '').trim().slice(0, 42) : null);
</script>

<footer class="footer">
  <div class="device">
    {#if shortName}
      <span class="dot ok" aria-hidden="true"></span>{shortName}
    {:else}
      <span class="dot" aria-hidden="true"></span>{tr('No controller: use the arrow keys, Enter and Esc')}
    {/if}
  </div>
  <ul class="legend" aria-label={tr('Button legend')}>
    {#each hints as h (h.key)}
      <li><kbd>{h.key}</kbd><span>{tr(h.label)}</span></li>
    {/each}
  </ul>
</footer>

<style>
  .footer {
    height: var(--footer-h);
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 var(--s5);
    border-top: 1px solid var(--linea);
    background: var(--asfalto-2);
    font-size: var(--fs-sm);
    color: var(--gesso-2);
  }
  .device {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--gesso-3);
  }
  .dot.ok {
    background: var(--verde);
  }
  .legend {
    display: flex;
    gap: var(--s5);
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  kbd {
    font-family: inherit;
    font-size: var(--fs-xs);
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 4px;
    background: var(--pannello-2);
    border: 1px solid var(--linea-forte);
    color: var(--gesso);
  }
</style>
