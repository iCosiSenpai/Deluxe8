<script lang="ts">
  /** Progress drawn as a track kerb: red/white blocks fill up from the left. */
  let { value = 0, label }: { value: number; label: string } = $props();
  const clamped = $derived(Math.max(0, Math.min(1, value)));
</script>

<div
  class="kerb"
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={Math.round(clamped * 100)}
>
  <div class="fill" style="width: {clamped * 100}%"></div>
</div>

<style>
  .kerb {
    height: 10px;
    background: var(--linea);
    border-radius: 2px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: repeating-linear-gradient(
      -60deg,
      var(--cordolo-a) 0 14px,
      var(--cordolo-b) 14px 28px
    );
    transition: width var(--t-med) var(--ease);
  }
</style>
