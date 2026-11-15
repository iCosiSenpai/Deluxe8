<script lang="ts">
  import { app } from '../state.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.tone}">{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: var(--s5);
    bottom: calc(var(--footer-h) + var(--s4));
    display: grid;
    gap: var(--s2);
    z-index: 50;
    max-width: 420px;
  }
  .toast {
    background: var(--pannello-2);
    border: 1px solid var(--linea-forte);
    border-left-width: 4px;
    padding: var(--s3) var(--s4);
    border-radius: var(--r-ctl);
    font-size: var(--fs-sm);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.35);
  }
  .ok { border-left-color: var(--verde); }
  .warn { border-left-color: var(--giallo); }
  .error { border-left-color: var(--rosso); }
  .info { border-left-color: var(--viola); }
</style>
