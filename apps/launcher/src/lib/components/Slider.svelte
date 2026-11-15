<script lang="ts">
  let {
    value = $bindable(),
    min = 0,
    max = 100,
    step = 1,
    label,
    format = (v: number) => String(v),
    onchange,
  }: {
    value: number;
    min?: number;
    max?: number;
    step?: number;
    label: string;
    format?: (v: number) => string;
    onchange?: (v: number) => void;
  } = $props();

  const pct = $derived(((value - min) / (max - min)) * 100);
</script>

<label class="slider" style="--pct: {pct}%">
  <span class="sr-only">{label}</span>
  <input
    type="range"
    {min}
    {max}
    {step}
    bind:value
    oninput={() => onchange?.(value)}
  />
  <output class="data">{format(value)}</output>
</label>

<style>
  .slider {
    display: flex;
    align-items: center;
    gap: var(--s3);
    width: 100%;
    max-width: 360px;
  }
  input {
    flex: 1;
    appearance: none;
    height: 28px;
    background: transparent;
    cursor: pointer;
  }
  input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--gesso) var(--pct), var(--linea) var(--pct));
  }
  input::-webkit-slider-thumb {
    appearance: none;
    width: 18px;
    height: 18px;
    margin-top: -7px;
    border-radius: 3px;
    transform: rotate(45deg);
    background: var(--gesso);
    border: 3px solid var(--asfalto);
  }
  input:focus-visible {
    box-shadow: none;
  }
  input:focus-visible::-webkit-slider-thumb {
    box-shadow: 0 0 0 3px var(--viola);
  }
  output {
    min-width: 4.2em;
    text-align: right;
    font-size: var(--fs-md);
  }
</style>
