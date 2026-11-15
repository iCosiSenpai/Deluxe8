<script lang="ts">
  import type { Snippet } from 'svelte';

  /** One line of the setup sheet: what it is, what it costs, and the control. */
  let {
    label,
    description = '',
    cost = null,
    tag = null,
    tone = null,
    stacked = false,
    children,
  }: {
    label: string;
    description?: string;
    /** GPU cost 1..3 (shown as a three-segment meter) */
    cost?: 1 | 2 | 3 | null;
    tag?: string | null;
    tone?: 'viola' | 'verde' | 'giallo' | 'rosso' | null;
    stacked?: boolean;
    children: Snippet;
  } = $props();
</script>

{#snippet meter(level: 1 | 2 | 3)}
  <div class="cost" title="Costo per la scheda video" aria-label="Costo: {level} su 3">
    {#each [1, 2, 3] as i (i)}
      <span class:on={i <= level} class="seg s{level}"></span>
    {/each}
  </div>
{/snippet}

<div class="row" class:stacked>
  <div class="text">
    <div class="head">
      <span class="label">{label}</span>
      {#if tag}<span class="tag {tone ?? ''}">{tag}</span>{/if}
      {#if cost && stacked}{@render meter(cost)}{/if}
    </div>
    {#if description}<p class="desc">{description}</p>{/if}
  </div>
  {#if !stacked}
    {#if cost}{@render meter(cost)}{:else}<div></div>{/if}
  {/if}
  <div class="control">{@render children()}</div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 40px minmax(260px, 380px);
    align-items: center;
    gap: var(--s5);
    padding: var(--s4) 0;
    border-bottom: 1px solid var(--linea);
  }
  .row.stacked {
    grid-template-columns: minmax(0, 1fr);
    gap: var(--s3);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  .label {
    font-weight: 600;
    font-size: var(--fs-md);
  }
  .desc {
    margin-top: 2px;
    color: var(--gesso-2);
    font-size: var(--fs-sm);
    max-width: 62ch;
    line-height: 1.45;
  }
  .tag {
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--pannello-2);
    color: var(--gesso-2);
  }
  .tag.viola { background: var(--viola-velo); color: #c9a9ff; }
  .tag.verde { background: var(--verde-velo); color: var(--verde); }
  .tag.giallo { background: var(--giallo-velo); color: var(--giallo); }
  .tag.rosso { background: var(--rosso-velo); color: #ff8a82; }
  .cost {
    display: flex;
    gap: 3px;
    justify-content: center;
  }
  .head .cost {
    margin-left: var(--s2);
  }
  .seg {
    width: 8px;
    height: 14px;
    border-radius: 2px;
    background: var(--linea);
    transform: skewX(-12deg);
  }
  .seg.on.s1 { background: var(--verde); }
  .seg.on.s2 { background: var(--giallo); }
  .seg.on.s3 { background: var(--rosso); }
  .control {
    display: flex;
    justify-content: flex-end;
  }
  .stacked .control {
    justify-content: flex-start;
  }
</style>
