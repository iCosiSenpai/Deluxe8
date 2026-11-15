<script lang="ts">
  import { onMount } from 'svelte';
  import TitleBar from './lib/components/TitleBar.svelte';
  import Tower from './lib/components/Tower.svelte';
  import Footer from './lib/components/Footer.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import Setup from './lib/pages/Setup.svelte';
  import Home from './lib/pages/Home.svelte';
  import Graphics from './lib/pages/Graphics.svelte';
  import Controls from './lib/pages/Controls.svelte';
  import Audio from './lib/pages/Audio.svelte';
  import Online from './lib/pages/Online.svelte';
  import Mods from './lib/pages/Mods.svelte';
  import System from './lib/pages/System.svelte';
  import { app, pages } from './lib/state.svelte';
  import { installGamepadNav, installKeyboardNav, moveFocus } from './lib/nav';
  import { demo } from './lib/api';

  let pad = $state<string | null>(null);

  onMount(() => {
    app.init().catch((e) => app.toast(`Avvio non riuscito: ${e}`, 'error', 10000));
    const back = () => {
      if (document.querySelector('[data-nav-scope]')) {
        window.dispatchEvent(new CustomEvent('d8:back'));
      } else if (app.page !== 'home' && app.settings.setupDone) {
        app.page = 'home';
      } else {
        window.dispatchEvent(new CustomEvent('d8:back'));
      }
    };
    const tab = (delta: number) => {
      if (!app.settings.setupDone) return;
      const i = pages.findIndex((p) => p.id === app.page);
      app.page = pages[(i + delta + pages.length) % pages.length].id;
    };
    installKeyboardNav(back, tab);
    installGamepadNav({
      back,
      tab,
      start: () => {
        if (app.settings.setupDone) app.page = 'home';
        setTimeout(() => document.querySelector<HTMLElement>('[data-autofocus]')?.focus(), 30);
      },
      connected: (name) => (pad = name),
    });
  });

  // Persist every settings change (deep read so nested edits count).
  $effect(() => {
    JSON.stringify(app.settings);
    app.scheduleSave();
  });

  // Keep a sensible focus target after page switches.
  $effect(() => {
    void app.page;
    setTimeout(() => {
      const el = document.activeElement;
      if ((!el || el === document.body) && !demo) moveFocus('down'); // demo pages (browser preview) open unfocused
    }, 40);
  });
</script>

<div class="shell">
  <TitleBar subtitle={app.settings.setupDone ? '' : 'Preparazione'} />
  {#if !app.loaded}
    <main class="loading"><p class="muted">Caricamento…</p></main>
  {:else if !app.settings.setupDone}
    <main class="setup"><Setup /></main>
  {:else}
    <div class="body">
      <Tower />
      <main class="page" id="page">
        {#key app.page}
          {#if app.page === 'home'}<Home />
          {:else if app.page === 'grafica'}<Graphics />
          {:else if app.page === 'controlli'}<Controls />
          {:else if app.page === 'audio'}<Audio />
          {:else if app.page === 'online'}<Online />
          {:else if app.page === 'mod'}<Mods />
          {:else if app.page === 'sistema'}<System />
          {/if}
        {/key}
      </main>
    </div>
  {/if}
  <Footer {pad} />
  <Toasts />
</div>

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .page,
  .setup {
    flex: 1;
    overflow-y: auto;
    min-width: 0;
  }
  .loading {
    flex: 1;
    display: grid;
    place-items: center;
  }
</style>
