<script lang="ts">
  import { tr } from '../i18n.svelte';
  import { onMount } from 'svelte';
  import { app } from '../state.svelte';
  import SettingRow from '../components/SettingRow.svelte';
  import Segmented from '../components/Segmented.svelte';
  import Toggle from '../components/Toggle.svelte';

  const s = app.settings;
  let pads = $state<{ index: number; id: string; buttons: number }[]>([]);

  onMount(() => {
    let raf = 0;
    let seen = '';
    const tick = () => {
      const now = [...(navigator.getGamepads?.() ?? [])]
        .filter((p): p is Gamepad => !!p && p.connected)
        .map((p) => ({ index: p.index, id: p.id, buttons: p.buttons.length }));
      const key = JSON.stringify(now);
      if (key !== seen) {
        seen = key; // a new list (and a re-render) only when a controller comes or goes
        pads = now;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
</script>

<section class="page">
  <h1 class="display">{tr('Controls')}</h1>
  <p class="lead">
    {tr('Controllers are recognised and mapped by themselves, Pro Controller, Xbox, DualSense and 8BitDo included. The first controller connected is player 1.')}
  </p>

  <h2>{tr('Connected now')}</h2>
  <div class="sheet">
    {#if pads.length}
      {#each pads as p (p.index)}
        <div class="pad">
          <span class="slot display">{tr('P{n}', { n: p.index + 1 })}</span>
          <span class="name">{p.id.replace(/\(.*?\)/g, '').trim()}</span>
          <span class="quiet data">{tr('{n} buttons', { n: p.buttons })}</span>
        </div>
      {/each}
    {:else}
      <p class="empty">{tr('No controller found. Connect one, or press a button to wake it up.')}</p>
    {/if}
  </div>

  <h2>{tr('Match')}</h2>
  <div class="sheet">
    <SettingRow label={tr('Players on this PC')} description={tr('With 2 to 4 players the screen splits as on the console.')}>
      <Segmented
        label={tr('Players')}
        bind:value={s.controls.players}
        options={[1, 2, 3, 4].map((n) => ({ value: n, label: String(n) }))}
      />
    </SettingRow>
    <SettingRow
      label={tr('60 fps split screen')}
      description={tr('With 3 or 4 players the game draws each view at 30 fps: this patch makes it 60 (update 4.0.1 only). Heavy on the CPU; the Native engine needs a new recompilation, which the preparation starts.')}
    >
      <Toggle label={tr('60 fps split screen')} bind:checked={s.controls.split60} />
    </SettingRow>
    <SettingRow label={tr('Rumble')} description={tr('HD Rumble translated for your controller.')}>
      <Toggle label={tr('Rumble')} bind:checked={s.controls.rumble} />
    </SettingRow>
    <SettingRow label={tr('Motion sensor')} description={tr('For gyro steering, if your controller has one.')}>
      <Toggle label={tr('Motion sensor')} bind:checked={s.controls.motion} />
    </SettingRow>
  </div>

  <h2>{tr('Quick menu in game')}</h2>
  <div class="sheet combo">
    <div class="keys">
      <kbd>L3</kbd><span>+</span><kbd>R3</kbd>
    </div>
    <p>
      {tr('Press both sticks together to open the Deluxe8 menu during a race: scaling filter, sharpness, frame generation, volume, online room and quit. On a keyboard use F1. While the menu is open the game gets no input.')}
    </p>
  </div>
</section>

<style>
  .page {
    padding: var(--s6) var(--s7) var(--s7);
    max-width: 1120px;
  }
  h1 {
    font-size: var(--fs-3xl);
    margin-bottom: var(--s3);
  }
  .lead {
    color: var(--gesso-2);
    max-width: 70ch;
  }
  h2 {
    font-size: var(--fs-lg);
    font-weight: 760;
    margin: var(--s6) 0 var(--s2);
  }
  .sheet {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: 0 var(--s5);
  }
  .pad {
    display: grid;
    grid-template-columns: 56px 1fr auto;
    align-items: center;
    gap: var(--s4);
    padding: var(--s4) 0;
    border-bottom: 1px solid var(--linea);
  }
  .pad:last-child {
    border-bottom: 0;
  }
  .slot {
    font-size: var(--fs-xl);
  }
  .empty {
    padding: var(--s5) 0;
    color: var(--gesso-2);
  }
  .combo {
    display: flex;
    align-items: center;
    gap: var(--s6);
    padding: var(--s5);
  }
  .combo p {
    color: var(--gesso-2);
    max-width: 62ch;
  }
  .keys {
    display: flex;
    align-items: center;
    gap: var(--s2);
    font-size: var(--fs-xl);
  }
  kbd {
    font-family: inherit;
    font-weight: 800;
    font-style: italic;
    padding: 4px 10px;
    border-radius: 6px;
    background: var(--gesso);
    color: var(--asfalto);
  }
</style>
