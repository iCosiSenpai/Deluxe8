<script lang="ts">
  import { tr } from '../i18n.svelte';
  import { isTauri } from '../api';

  let { subtitle = '' }: { subtitle?: string } = $props();

  async function win() {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow();
  }
  const minimize = async () => (await win()).minimize();
  const toggleMax = async () => (await win()).toggleMaximize();
  const close = async () => (await win()).close();
</script>

<header class="bar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="mark display" aria-hidden="true">D8</span>
    <span class="name" data-tauri-drag-region>Deluxe8</span>
    {#if subtitle}<span class="sub" data-tauri-drag-region>{subtitle}</span>{/if}
  </div>
  {#if isTauri}
    <div class="controls" data-nav-skip>
      <button tabindex="-1" aria-label={tr('Minimize')} onclick={minimize}>
        <svg viewBox="0 0 12 12"><path d="M2 6.5h8" /></svg>
      </button>
      <button tabindex="-1" aria-label={tr('Maximize')} onclick={toggleMax}>
        <svg viewBox="0 0 12 12"><rect x="2.5" y="2.5" width="7" height="7" /></svg>
      </button>
      <button tabindex="-1" class="close" aria-label={tr('Close')} onclick={close}>
        <svg viewBox="0 0 12 12"><path d="M3 3l6 6M9 3l-6 6" /></svg>
      </button>
    </div>
  {/if}
</header>

<style>
  .bar {
    height: var(--titlebar-h);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: var(--s4);
    background: var(--asfalto);
    border-bottom: 1px solid var(--linea);
  }
  .brand {
    display: flex;
    align-items: baseline;
    gap: var(--s3);
  }
  .mark {
    font-size: 15px;
    padding: 1px 6px 0 4px;
    background: var(--gesso);
    color: var(--asfalto);
    border-radius: 3px;
    transform: skewX(-8deg);
  }
  .name {
    font-weight: 640;
    font-size: var(--fs-sm);
    letter-spacing: 0.01em;
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--gesso-3);
  }
  .controls {
    display: flex;
    height: 100%;
  }
  .controls button {
    width: 46px;
    height: 100%;
    border: 0;
    background: transparent;
    display: grid;
    place-items: center;
    color: var(--gesso-2);
  }
  .controls button:hover {
    background: var(--pannello-2);
    color: var(--gesso);
  }
  .controls button.close:hover {
    background: var(--rosso);
    color: #fff;
  }
  svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
  }
</style>
