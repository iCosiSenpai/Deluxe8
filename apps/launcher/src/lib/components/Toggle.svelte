<script lang="ts">
  import { tr } from '../i18n.svelte';
  let {
    checked = $bindable(),
    label,
    onchange,
  }: { checked: boolean; label: string; onchange?: (v: boolean) => void } = $props();
</script>

<button
  class="toggle"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  onclick={() => {
    checked = !checked;
    onchange?.(checked);
  }}
>
  <span class="track"><span class="knob"></span></span>
  <span class="state">{checked ? tr('On') : tr('Off')}</span>
</button>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--s3);
    border: 0;
    background: transparent;
    padding: 4px;
    border-radius: var(--r-ctl);
    color: var(--gesso-2);
    font-size: var(--fs-sm);
  }
  .track {
    width: 46px;
    height: 24px;
    border-radius: 999px;
    background: var(--linea);
    position: relative;
    transition: background var(--t-fast) var(--ease);
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--gesso-2);
    transition: transform var(--t-med) var(--ease), background var(--t-fast);
  }
  [aria-checked='true'] .track {
    background: var(--verde);
  }
  [aria-checked='true'] .knob {
    transform: translateX(22px);
    background: var(--asfalto);
  }
  [aria-checked='true'] .state {
    color: var(--gesso);
  }
  .state {
    min-width: 4.5em;
    text-align: left;
  }
</style>
