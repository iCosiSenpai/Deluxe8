<script lang="ts">
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
  <h1 class="display">Controlli</h1>
  <p class="lead">
    I controller vengono riconosciuti e mappati da soli, compresi Pro Controller, Xbox, DualSense e 8BitDo.
    Il primo controller collegato è il giocatore 1.
  </p>

  <h2>Collegati ora</h2>
  <div class="sheet">
    {#if pads.length}
      {#each pads as p (p.index)}
        <div class="pad">
          <span class="slot display">G{p.index + 1}</span>
          <span class="name">{p.id.replace(/\(.*?\)/g, '').trim()}</span>
          <span class="quiet data">{p.buttons} tasti</span>
        </div>
      {/each}
    {:else}
      <p class="empty">Nessun controller rilevato. Collegane uno o premi un tasto per svegliarlo.</p>
    {/if}
  </div>

  <h2>Partita</h2>
  <div class="sheet">
    <SettingRow label="Giocatori sullo stesso PC" description="Da 2 a 4 lo schermo si divide come sulla console.">
      <Segmented
        label="Giocatori"
        bind:value={s.controls.players}
        options={[1, 2, 3, 4].map((n) => ({ value: n, label: String(n) }))}
      />
    </SettingRow>
    <SettingRow
      label="60 fps a schermo diviso"
      description="Con 3 o 4 giocatori il gioco disegna ogni riquadro a 30 fps: questa modifica li porta a 60 (solo aggiornamento 4.0.1). Pesa molto sulla CPU; nel motore Nativo serve una nuova ricompilazione, che parte dalla preparazione."
    >
      <Toggle label="60 fps a schermo diviso" bind:checked={s.controls.split60} />
    </SettingRow>
    <SettingRow label="Vibrazione" description="HD Rumble tradotto per il tuo controller.">
      <Toggle label="Vibrazione" bind:checked={s.controls.rumble} />
    </SettingRow>
    <SettingRow label="Sensore di movimento" description="Per la guida col giroscopio, se il controller lo ha.">
      <Toggle label="Sensore di movimento" bind:checked={s.controls.motion} />
    </SettingRow>
  </div>

  <h2>Menu rapido in gioco</h2>
  <div class="sheet combo">
    <div class="keys">
      <kbd>L3</kbd><span>+</span><kbd>R3</kbd>
    </div>
    <p>
      Premi insieme le due levette per aprire il menu Deluxe8 durante la partita: filtro di scala, nitidezza,
      frame generation, volume, stanza online ed uscita. Da tastiera usa <kbd>F1</kbd>. Mentre il menu è aperto il
      gioco non riceve comandi.
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
  p kbd {
    font-size: var(--fs-sm);
    padding: 1px 6px;
  }
</style>
