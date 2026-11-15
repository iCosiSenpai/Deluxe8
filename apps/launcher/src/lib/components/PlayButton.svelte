<script lang="ts">
  /**
   * The one orchestrated moment: five start lights come on one by one, go out together,
   * and the race starts. Reduced motion skips straight to the launch.
   */
  let {
    running = false,
    disabled = false,
    reason = '',
    onlaunch,
    onstop,
  }: {
    running?: boolean;
    disabled?: boolean;
    reason?: string;
    onlaunch: () => void;
    onstop: () => void;
  } = $props();

  let lit = $state(0);
  let sequencing = $state(false);
  const reduced =
    typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;

  async function start() {
    if (sequencing || disabled) return;
    if (reduced) {
      onlaunch();
      return;
    }
    sequencing = true;
    for (let i = 1; i <= 5; i++) {
      lit = i;
      await new Promise((r) => setTimeout(r, 170));
    }
    await new Promise((r) => setTimeout(r, 320));
    lit = 0;
    sequencing = false;
    onlaunch();
  }
</script>

<div class="start" class:running>
  <div class="gantry" aria-hidden="true">
    {#each [1, 2, 3, 4, 5] as i (i)}
      <span class="pod">
        <span class="lamp" class:on={lit >= i}></span>
        <span class="lamp" class:on={lit >= i}></span>
      </span>
    {/each}
  </div>

  {#if running}
    <div class="race">
      <span class="live" aria-hidden="true"></span>
      <span class="race-label">In gara</span>
      <button class="stop" onclick={onstop}>Termina il gioco</button>
    </div>
  {:else}
    <button class="go display" data-autofocus {disabled} onclick={start} aria-describedby="go-reason">
      {sequencing ? 'Pronti…' : 'Gioca'}
    </button>
    {#if disabled && reason}<p id="go-reason" class="reason">{reason}</p>{/if}
  {/if}
</div>

<style>
  .start {
    display: grid;
    gap: var(--s4);
    justify-items: start;
  }
  .gantry {
    display: flex;
    gap: 10px;
    padding: 10px 14px;
    background: #0d1118;
    border: 1px solid var(--linea);
    border-radius: 8px;
  }
  .pod {
    display: grid;
    gap: 6px;
    padding: 6px;
    background: #090c12;
    border-radius: 6px;
  }
  .lamp {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: #2a1114;
    box-shadow: inset 0 0 0 2px #1a0b0d;
    transition: background 60ms linear, box-shadow 60ms linear;
  }
  .lamp.on {
    background: var(--rosso);
    box-shadow: 0 0 14px 2px rgb(255 59 48 / 0.65), inset 0 0 0 2px #ff6a5f;
  }
  .running .lamp {
    background: #102a1b;
  }
  .go {
    font-size: clamp(2.2rem, 4.2vw, 3.4rem);
    padding: 0.32em 1.5em 0.36em 1.25em;
    border: 0;
    color: var(--asfalto);
    background: var(--gesso);
    clip-path: polygon(0 0, calc(100% - 26px) 0, 100% 50%, calc(100% - 26px) 100%, 0 100%);
    transition: transform var(--t-fast) var(--ease), background var(--t-fast);
  }
  .go:hover:not(:disabled) {
    background: #ffffff;
    transform: translateX(3px);
  }
  .go:focus-visible {
    box-shadow: none;
    outline: 4px solid var(--viola);
    outline-offset: 4px;
  }
  .go:disabled {
    background: var(--linea-forte);
    color: var(--gesso-3);
    cursor: not-allowed;
  }
  .reason {
    color: var(--giallo);
    font-size: var(--fs-sm);
    max-width: 48ch;
  }
  .race {
    display: flex;
    align-items: center;
    gap: var(--s4);
  }
  .live {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--verde);
    box-shadow: 0 0 0 5px var(--verde-velo);
  }
  .race-label {
    font-size: var(--fs-2xl);
    font-style: italic;
    font-weight: 860;
    font-stretch: 125%;
  }
  .stop {
    border: 1px solid var(--linea-forte);
    background: transparent;
    padding: 8px 14px;
    border-radius: var(--r-ctl);
    font-weight: 600;
  }
  .stop:hover {
    border-color: var(--rosso);
    color: #ff8a82;
  }
</style>
