<script lang="ts">
  import { onMount } from 'svelte';
  import { api, on, openUrl } from '../api';
  import { tr } from '../i18n.svelte';
  import { app } from '../state.svelte';
  import SettingRow from '../components/SettingRow.svelte';
  import Slider from '../components/Slider.svelte';
  import Toggle from '../components/Toggle.svelte';
  import Kerb from '../components/Kerb.svelte';
  import type { ComponentDone, ComponentsStatus, PrepareStep, RoomInfo } from '../types';

  const s = app.settings;
  const GAME_VERSION = '4.0.1';
  const NEXTENDO_URL = 'https://nextendo.network/';
  let rooms = $state<RoomInfo[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let onlyMine = $state(true);
  let hosting = $state<{ address: string; port: number; upnp: string } | null>(null);
  let joinPassword = $state('');
  let askPassword = $state<RoomInfo | null>(null);
  let comp = $state<ComponentsStatus | null>(null);
  let installing = $state(false);
  let progress = $state<PrepareStep | null>(null);
  let opening = $state(false);

  const nicknameOk = $derived(/^[a-zA-Z0-9._\- ]{4,20}$/.test(s.online.nickname));
  const visible = $derived(
    rooms.filter((r) => !onlyMine || r.gameVersion === null || r.gameVersion === GAME_VERSION),
  );
  const compat = $derived(s.engine === 'compat');
  const clientReady = $derived(!!comp?.onlineEngine);
  const signedIn = $derived(!!comp?.nextendoSignedIn);

  async function refreshComponents() {
    comp = await api.componentsStatus().catch(() => comp);
    app.system = await api.systemInfo().catch(() => app.system);
  }

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
      app.toast(tr('Pick a driver name first (4 to 20 characters).'), 'warn');
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
      app.toast(tr('Could not join: {error}', { error: String(e) }), 'error');
    }
  }

  async function host() {
    if (!nicknameOk) {
      app.toast(tr('Pick a driver name first (4 to 20 characters).'), 'warn');
      return;
    }
    try {
      hosting = await api.hostRoom();
      app.toast(tr('Room open. Start the game and choose Wireless Play.'), 'ok');
    } catch (e) {
      app.toast(tr('Room not opened: {error}', { error: String(e) }), 'error');
    }
  }

  async function stopHosting() {
    await api.stopRoom();
    hosting = null;
  }

  async function installClient() {
    installing = true;
    progress = null;
    try {
      await api.installComponent('online');
    } catch (e) {
      installing = false;
      app.toast(String(e), 'error');
    }
  }

  async function signIn() {
    opening = true;
    try {
      await api.openOnlineClient();
      app.toast(tr('In Ryujinx Nextendo open Settings > Nextendo, sign in, then close the window.'), 'info', 12000);
    } catch (e) {
      app.toast(String(e), 'error');
    } finally {
      opening = false;
    }
  }

  async function playOnline() {
    try {
      s.engine = 'online';
      await api.launch('online');
      app.page = 'home';
    } catch (e) {
      app.toast(tr('Could not start: {error}', { error: String(e) }), 'error', 9000);
    }
  }

  onMount(() => {
    refresh();
    refreshComponents();
    api.roomStatus().then((st) => {
      if (st.hosting && st.address && st.port) hosting = { address: st.address, port: st.port, upnp: '' };
    });
    const offs = [
      on<PrepareStep>('prepare://step', (p) => {
        if (installing) progress = p;
      }),
      on<ComponentDone>('component://done', (d) => {
        if (d.name !== 'online') return;
        installing = false;
        progress = null;
        if (d.ok) app.toast(tr('Ryujinx Nextendo is ready'), 'ok');
        else app.toast(d.message ?? tr('Download failed'), 'error', 8000);
        refreshComponents();
      }),
      // back from the sign-in window: show the account
      on('online://closed', () => refreshComponents()),
    ];
    const back = () => (askPassword = null);
    window.addEventListener('d8:back', back);
    return () => {
      offs.forEach((p) => p.then((f) => f()));
      window.removeEventListener('d8:back', back);
    };
  });
</script>

<section class="page">
  <h1 class="display">{tr('Online')}</h1>
  <p class="lead">
    {tr('Two ways to race online, both with your own copy of the game and neither through Nintendo: the Nextendo Network for online races with players everywhere, and rooms for local wireless races over the internet.')}
  </p>

  <h2>Nextendo Network</h2>
  <div class="sheet nx">
    <p class="nx-lead">
      {tr("Worldwide and regional races, tournaments, friends and time-trial ghosts, like the original service. Nextendo Network is a free, community-run replacement: you race against players on consoles and on the Nextendo builds of Ryujinx and Citron. Deluxe8 runs the game in Ryujinx Nextendo for this.")}
    </p>
    <ol class="steps">
      <li class:done={clientReady}>
        <span class="num display">1</span>
        <div class="step-text">
          <p class="step-title">{tr('Get Ryujinx Nextendo')}</p>
          <p class="step-sub">
            {clientReady
              ? tr('Version {version} is on this PC.', { version: comp?.onlineVersion ?? '' })
              : tr('The official build from the Nextendo team ({mb} MB, checked by SHA-256). Your files stay where they are.', { mb: comp?.onlineDownloadMb ?? 42 })}
          </p>
          {#if installing && progress}
            <Kerb value={progress.progress ?? 0} label={tr('Download')} />
            <p class="step-sub">{progress.detail ?? ''}</p>
          {/if}
        </div>
        {#if !clientReady}
          <button class="primary" onclick={installClient} disabled={installing}>{installing ? tr('Downloading…') : tr('Download')}</button>
        {/if}
      </li>
      <li class:done={signedIn}>
        <span class="num display">2</span>
        <div class="step-text">
          <p class="step-title">{tr('Create a free Nextendo account')}</p>
          <p class="step-sub">{tr('Three fields on nextendo.network: you get a username and a permanent friend code.')}</p>
        </div>
        {#if !signedIn}
          <button class="ghost" onclick={() => openUrl(NEXTENDO_URL)}>{tr('Create account')}</button>
        {/if}
      </li>
      <li class:done={signedIn}>
        <span class="num display">3</span>
        <div class="step-text">
          <p class="step-title">{tr('Sign in')}</p>
          <p class="step-sub">
            {signedIn
              ? tr('Signed in: your Nextendo account is linked in Ryujinx Nextendo.')
              : tr('Opens Ryujinx Nextendo: under Settings > Nextendo enter your e-mail and password, then close the window. Deluxe8 never sees your password.')}
          </p>
        </div>
        <button class="ghost" onclick={signIn} disabled={!clientReady || opening}>{signedIn ? tr('Change account') : tr('Sign in')}</button>
      </li>
      <li>
        <span class="num display">4</span>
        <div class="step-text">
          <p class="step-title">{tr('Race online')}</p>
          <p class="step-sub">{tr('Starts the game in Ryujinx Nextendo with your update and DLC; choose Online play in the game menu. Without an account the game still starts, offline.')}</p>
        </div>
        <button class="primary" onclick={playOnline} disabled={!clientReady || app.launch.running}>{tr('Play online')}</button>
      </li>
    </ol>
  </div>
  <p class="aside">
    {tr('Ryujinx Nextendo is an independent emulator (MIT licence) by the Nextendo team. Deluxe8 downloads their official build, prepares it with your keys, firmware and game, and starts it; its graphics settings are in its own window. Nextendo is not affiliated with Nintendo.')}
  </p>

  <h2>{tr('Rooms')}</h2>
  <p class="lead">
    {tr('A room links the local wireless mode (up to 8 drivers) or the LAN mode (up to 12: press L + R + left stick on the title menu) over the internet. Rooms work in the Emulator engine and with other emulators of the yuzu family (Eden, Citron, suyu, Sudachi): their players join your room with the same address.')}
  </p>
  {#if !compat}
    <p class="banner">{tr('Rooms use the Emulator engine: joining one selects it for you.')}</p>
  {/if}

  <h3 class="sub-h">{tr('Your driver')}</h3>
  <div class="sheet">
    <SettingRow label={tr('Name')} description={tr('Shown to others in the room. 4 to 20 characters: letters, digits, space, . _ -')}>
      <input
        class="text"
        class:bad={s.online.nickname.length > 0 && !nicknameOk}
        bind:value={s.online.nickname}
        maxlength="20"
        placeholder={tr('Driver name')}
        aria-label={tr('Driver name')}
      />
    </SettingRow>
  </div>

  <div class="rooms-head">
    <h3 class="sub-h">{tr('Open rooms')}</h3>
    <div class="tools">
      <label class="check"><input type="checkbox" bind:checked={onlyMine} /> {tr('Version {version} only', { version: GAME_VERSION })}</label>
      <button class="ghost" onclick={refresh} disabled={loading}>{loading ? tr('Refreshing…') : tr('Refresh')}</button>
    </div>
  </div>
  <div class="sheet list">
    {#if error}
      <p class="empty">{tr('List not available: {error}', { error })}</p>
    {:else if !visible.length && !loading}
      <p class="empty">{tr('No open rooms for this version. Open one below.')}</p>
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
        <span class="lock" aria-label={r.hasPassword ? tr('With password') : tr('Open to all')}>{r.hasPassword ? tr('Password') : ''}</span>
        <button
          class="primary"
          disabled={r.players >= r.maxPlayers || (r.gameVersion !== null && r.gameVersion !== GAME_VERSION)}
          onclick={() => join(r)}
        >
          {tr('Join')}
        </button>
      </div>
    {/each}
  </div>

  <h3 class="sub-h">{tr('Open a room')}</h3>
  <div class="sheet">
    {#if hosting}
      <div class="hosting">
        <span class="live" aria-hidden="true"></span>
        <div>
          <p class="room-name">{tr('Room open on')} <span class="data">{hosting.address}:{hosting.port}</span></p>
          {#if hosting.upnp}<p class="room-sub">{hosting.upnp}</p>{/if}
        </div>
        <button class="ghost" onclick={stopHosting}>{tr('Close the room')}</button>
      </div>
    {:else}
      <SettingRow label={tr('Room name')} description={tr('How others will see it.')}>
        <input class="text" bind:value={s.online.roomName} maxlength="40" aria-label={tr('Room name')} />
      </SettingRow>
      <SettingRow label={tr('Seats')} description={tr('8 for local wireless, up to 12 in LAN mode.')}>
        <Slider label={tr('Seats')} bind:value={s.online.maxPlayers} min={2} max={12} step={1} />
      </SettingRow>
      <SettingRow label={tr('Password')} description={tr('Leave it empty for a room open to everyone.')}>
        <input class="text" type="password" bind:value={s.online.password} maxlength="32" aria-label={tr('Room password')} />
      </SettingRow>
      <SettingRow label={tr('Open the port on the router')} description={tr('Uses UPnP so the room can be reached from the internet (port {port}).', { port: s.online.hostPort })}>
        <Toggle label="UPnP" bind:checked={s.online.upnp} />
      </SettingRow>
      <div class="host-actions">
        <button class="primary" onclick={host}>{tr('Open the room')}</button>
      </div>
    {/if}
  </div>
</section>

{#if askPassword}
  <div class="modal" data-nav-scope role="dialog" aria-modal="true" aria-label={tr('Room password')}>
    <div class="dialog">
      <h3>{askPassword.name}</h3>
      <p class="room-sub">{tr('This room is password protected.')}</p>
      <input class="text" type="password" bind:value={joinPassword} placeholder={tr('Password')} aria-label={tr('Password')} data-autofocus />
      <div class="host-actions">
        <button class="ghost" onclick={() => (askPassword = null)}>{tr('Cancel')}</button>
        <button class="primary" onclick={() => askPassword && join(askPassword, joinPassword)}>{tr('Join')}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .sub-h {
    font-size: var(--fs-md);
    font-weight: 700;
    color: var(--gesso-2);
    margin: var(--s5) 0 var(--s2);
  }
  .sheet.nx {
    padding: var(--s5);
  }
  .nx-lead {
    color: var(--gesso-2);
    max-width: 78ch;
    margin: 0 0 var(--s4);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--s2);
  }
  .steps li {
    display: grid;
    grid-template-columns: 36px 1fr auto;
    align-items: center;
    gap: var(--s4);
    padding: var(--s3) 0;
    border-top: 1px solid var(--linea);
  }
  .num {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    border: 1px solid var(--linea-forte);
    color: var(--gesso-2);
  }
  .steps li.done .num {
    background: var(--verde-velo);
    border-color: var(--verde);
    color: var(--verde);
  }
  .step-title {
    font-weight: 650;
    margin: 0;
  }
  .step-sub {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    margin: 2px 0 0;
    max-width: 70ch;
  }
  .aside {
    color: var(--gesso-3);
    font-size: var(--fs-sm);
    max-width: 90ch;
    margin: var(--s2) 0 0;
  }
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
