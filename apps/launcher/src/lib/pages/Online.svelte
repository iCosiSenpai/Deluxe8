<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../api';
  import { app } from '../state.svelte';
  import SettingRow from '../components/SettingRow.svelte';
  import Slider from '../components/Slider.svelte';
  import Toggle from '../components/Toggle.svelte';
  import type { RoomInfo } from '../types';

  const s = app.settings;
  const GAME_VERSION = '4.0.1';
  let rooms = $state<RoomInfo[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let onlyMine = $state(true);
  let hosting = $state<{ address: string; port: number; upnp: string } | null>(null);
  let joinPassword = $state('');
  let askPassword = $state<RoomInfo | null>(null);

  const nicknameOk = $derived(/^[a-zA-Z0-9._\- ]{4,20}$/.test(s.online.nickname));
  const visible = $derived(
    rooms.filter((r) => !onlyMine || r.gameVersion === null || r.gameVersion === GAME_VERSION),
  );
  const compat = $derived(s.engine === 'compat');

  async function refresh() {
    loading = true;
    error = null;
    try {
      rooms = await api.listRooms();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function join(r: RoomInfo, password = '') {
    if (!nicknameOk) {
      app.toast('Scegli prima un nome da pilota (4–20 caratteri).', 'warn');
      return;
    }
    if (r.hasPassword && !password) {
      askPassword = r;
      return;
    }
    askPassword = null;
    try {
      s.engine = 'compat';
      await api.launch('compat', { address: r.address, port: r.port, password });
      app.page = 'home';
    } catch (e) {
      app.toast(`Impossibile entrare: ${e}`, 'error');
    }
  }

  async function host() {
    if (!nicknameOk) {
      app.toast('Scegli prima un nome da pilota (4–20 caratteri).', 'warn');
      return;
    }
    try {
      hosting = await api.hostRoom();
      app.toast('Stanza aperta. Avvia il gioco e scegli Wireless locale.', 'ok');
    } catch (e) {
      app.toast(`Stanza non aperta: ${e}`, 'error');
    }
  }

  async function stopHosting() {
    await api.stopRoom();
    hosting = null;
  }

  onMount(() => {
    refresh();
    api.roomStatus().then((st) => {
      if (st.hosting && st.address && st.port) hosting = { address: st.address, port: st.port, upnp: '' };
    });
    const back = () => (askPassword = null);
    window.addEventListener('d8:back', back);
    return () => window.removeEventListener('d8:back', back);
  });
</script>

<section class="page">
  <h1 class="display">Online</h1>
  <p class="lead">
    Si gioca in <strong>stanze</strong>: il gioco crede di essere in Wireless locale e Deluxe8 collega le console
    virtuali via internet. Fino a 8 piloti in Wireless locale, fino a 12 in modalità LAN
    (nel menu principale del gioco premi L + R + levetta sinistra). Nessun collegamento ai server Nintendo.
  </p>
  {#if !compat}
    <p class="banner">Le stanze funzionano con il motore Compat: entrando in una stanza Deluxe8 lo seleziona per te.</p>
  {/if}

  <h2>Il tuo pilota</h2>
  <div class="sheet">
    <SettingRow label="Nome" description="Visibile agli altri nella stanza. Da 4 a 20 caratteri: lettere, numeri, spazio, . _ -">
      <input
        class="text"
        class:bad={s.online.nickname.length > 0 && !nicknameOk}
        bind:value={s.online.nickname}
        maxlength="20"
        placeholder="Nome da pilota"
        aria-label="Nome da pilota"
      />
    </SettingRow>
  </div>

  <div class="rooms-head">
    <h2>Stanze aperte</h2>
    <div class="tools">
      <label class="check"><input type="checkbox" bind:checked={onlyMine} /> Solo versione {GAME_VERSION}</label>
      <button class="ghost" onclick={refresh} disabled={loading}>{loading ? 'Aggiorno…' : 'Aggiorna'}</button>
    </div>
  </div>
  <div class="sheet list">
    {#if error}
      <p class="empty">Elenco non disponibile: {error}</p>
    {:else if !visible.length && !loading}
      <p class="empty">Nessuna stanza aperta per questa versione. Aprine una qui sotto.</p>
    {/if}
    {#each visible as r (r.id)}
      <div class="room">
        <div class="room-main">
          <span class="room-name">{r.name}</span>
          <span class="room-sub">{r.owner}{r.description ? ` — ${r.description}` : ''}</span>
        </div>
        <span class="version data" class:mismatch={r.gameVersion !== null && r.gameVersion !== GAME_VERSION}>
          {r.gameVersion ?? '?'}
        </span>
        <span class="players data">{r.players}/{r.maxPlayers}</span>
        <span class="lock" aria-label={r.hasPassword ? 'Con password' : 'Aperta'}>{r.hasPassword ? 'Password' : ''}</span>
        <button
          class="primary"
          disabled={r.players >= r.maxPlayers || (r.gameVersion !== null && r.gameVersion !== GAME_VERSION)}
          onclick={() => join(r)}
        >
          Entra
        </button>
      </div>
    {/each}
  </div>

  <h2>Apri una stanza</h2>
  <div class="sheet">
    {#if hosting}
      <div class="hosting">
        <span class="live" aria-hidden="true"></span>
        <div>
          <p class="room-name">Stanza aperta su <span class="data">{hosting.address}:{hosting.port}</span></p>
          {#if hosting.upnp}<p class="room-sub">{hosting.upnp}</p>{/if}
        </div>
        <button class="ghost" onclick={stopHosting}>Chiudi la stanza</button>
      </div>
    {:else}
      <SettingRow label="Nome della stanza" description="Come la vedranno gli altri.">
        <input class="text" bind:value={s.online.roomName} maxlength="40" aria-label="Nome della stanza" />
      </SettingRow>
      <SettingRow label="Posti" description="8 per il Wireless locale, fino a 12 in modalità LAN.">
        <Slider label="Posti" bind:value={s.online.maxPlayers} min={2} max={12} step={1} />
      </SettingRow>
      <SettingRow label="Password" description="Lasciala vuota per una stanza aperta a tutti.">
        <input class="text" type="password" bind:value={s.online.password} maxlength="32" aria-label="Password della stanza" />
      </SettingRow>
      <SettingRow label="Apri la porta sul router" description="Usa UPnP per rendere la stanza raggiungibile da internet (porta {s.online.hostPort}).">
        <Toggle label="UPnP" bind:checked={s.online.upnp} />
      </SettingRow>
      <div class="host-actions">
        <button class="primary" onclick={host}>Apri la stanza</button>
      </div>
    {/if}
  </div>
</section>

{#if askPassword}
  <div class="modal" data-nav-scope role="dialog" aria-modal="true" aria-label="Password della stanza">
    <div class="dialog">
      <h3>{askPassword.name}</h3>
      <p class="room-sub">Questa stanza è protetta da password.</p>
      <input class="text" type="password" bind:value={joinPassword} placeholder="Password" aria-label="Password" data-autofocus />
      <div class="host-actions">
        <button class="ghost" onclick={() => (askPassword = null)}>Annulla</button>
        <button class="primary" onclick={() => askPassword && join(askPassword, joinPassword)}>Entra</button>
      </div>
    </div>
  </div>
{/if}

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
    max-width: 74ch;
  }
  .banner {
    margin-top: var(--s4);
    padding: var(--s3) var(--s4);
    border-left: 4px solid var(--giallo);
    background: var(--giallo-velo);
    border-radius: var(--r-ctl);
    max-width: 74ch;
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
  .text {
    width: 100%;
    max-width: 320px;
    background: var(--asfalto);
    border: 1px solid var(--linea-forte);
    border-radius: var(--r-ctl);
    padding: 9px 12px;
    user-select: text;
  }
  .text:focus-visible {
    border-color: var(--gesso);
  }
  .text.bad {
    border-color: var(--rosso);
  }
  .rooms-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: var(--s4);
    margin-bottom: var(--s2);
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--s2);
    color: var(--gesso-2);
    font-size: var(--fs-sm);
  }
  .room {
    display: grid;
    grid-template-columns: 1fr 70px 60px 80px auto;
    align-items: center;
    gap: var(--s4);
    padding: var(--s4) 0;
    border-bottom: 1px solid var(--linea);
  }
  .room:last-child {
    border-bottom: 0;
  }
  .room-main {
    display: grid;
    min-width: 0;
  }
  .room-name {
    font-weight: 650;
  }
  .room-sub {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .version {
    color: var(--verde);
  }
  .version.mismatch {
    color: var(--giallo);
  }
  .lock {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
  }
  .empty {
    padding: var(--s5) 0;
    color: var(--gesso-2);
  }
  .hosting {
    display: flex;
    align-items: center;
    gap: var(--s4);
    padding: var(--s5) 0;
  }
  .hosting > div {
    flex: 1;
  }
  .live {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--verde);
    box-shadow: 0 0 0 5px var(--verde-velo);
  }
  .host-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--s3);
    padding: var(--s4) 0;
  }
  .primary,
  .ghost {
    border-radius: var(--r-ctl);
    padding: 8px 16px;
    font-weight: 650;
    border: 1px solid transparent;
  }
  .primary {
    background: var(--gesso);
    color: var(--asfalto);
  }
  .primary:disabled {
    background: var(--linea-forte);
    color: var(--gesso-3);
    cursor: not-allowed;
  }
  .ghost {
    background: transparent;
    border-color: var(--linea-forte);
  }
  .modal {
    position: fixed;
    inset: 0;
    background: rgb(8 10 16 / 0.7);
    display: grid;
    place-items: center;
    z-index: 40;
  }
  .dialog {
    background: var(--pannello);
    border: 1px solid var(--linea-forte);
    border-radius: var(--r-panel);
    padding: var(--s5);
    width: min(440px, 90vw);
    display: grid;
    gap: var(--s3);
  }
  .dialog h3 {
    font-size: var(--fs-lg);
  }
</style>
