<script lang="ts" generics="T extends string | number">
  /** Radio group as connected chips. Left/right (and the D-pad) change the value. */
  let {
    value = $bindable(),
    options,
    label,
    onchange,
  }: {
    value: T;
    options: { value: T; label: string; disabled?: boolean; recommended?: boolean }[];
    label: string;
    onchange?: (v: T) => void;
  } = $props();

  function select(v: T) {
    if (v === value) return;
    value = v;
    onchange?.(v);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    const enabled = options.filter((o) => !o.disabled);
    const i = enabled.findIndex((o) => o.value === value);
    const next = enabled[Math.max(0, Math.min(enabled.length - 1, i + (e.key === 'ArrowRight' ? 1 : -1)))];
    if (next) select(next.value);
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<div class="seg" role="radiogroup" aria-label={label} data-horizontal>
  {#each options as o (o.value)}
    <button
      role="radio"
      aria-checked={o.value === value}
      class:on={o.value === value}
      class:rec={o.recommended}
      disabled={o.disabled}
      tabindex={o.value === value ? 0 : -1}
      onclick={() => select(o.value)}
      {onkeydown}
    >
      {o.label}
      {#if o.recommended}<span class="sr-only">(consigliato)</span>{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    flex-wrap: wrap;
    background: var(--asfalto);
    border: 1px solid var(--linea);
    border-radius: var(--r-ctl);
    padding: 3px;
    gap: 3px;
  }
  button {
    position: relative;
    border: 0;
    background: transparent;
    color: var(--gesso-2);
    font-size: var(--fs-sm);
    font-weight: 560;
    padding: 6px 12px;
    border-radius: 4px;
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    color: var(--gesso);
    background: var(--pannello-2);
  }
  button.on {
    background: var(--gesso);
    color: var(--asfalto);
    font-weight: 700;
  }
  button.rec::after {
    content: '';
    position: absolute;
    top: 4px;
    right: 4px;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--viola);
  }
  button:disabled {
    color: var(--gesso-3);
    opacity: 0.5;
    cursor: not-allowed;
  }
  button:focus-visible {
    box-shadow: var(--focus-ring);
    z-index: 1;
  }
</style>
