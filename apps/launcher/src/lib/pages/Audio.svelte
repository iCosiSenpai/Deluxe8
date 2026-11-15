<script lang="ts">
  import { app } from '../state.svelte';
  import SettingRow from '../components/SettingRow.svelte';
  import Segmented from '../components/Segmented.svelte';
  import Slider from '../components/Slider.svelte';
  import Toggle from '../components/Toggle.svelte';

  const s = app.settings;
</script>

<section class="page">
  <h1 class="display">Audio</h1>
  <div class="sheet">
    <SettingRow label="Volume" description="Si regola anche in gioco dal menu L3 + R3.">
      <Slider label="Volume" bind:value={s.audio.volume} min={0} max={100} step={5} format={(v) => `${v}%`} />
    </SettingRow>
    <SettingRow label="Silenzia quando la finestra non è attiva" description="Utile se giochi in finestra.">
      <Toggle label="Silenzia in background" bind:checked={s.audio.muteInBackground} />
    </SettingRow>
    <SettingRow label="Uscita audio" description="Automatica sceglie la più stabile per il tuo sistema.">
      <Segmented
        label="Uscita audio"
        bind:value={s.audio.output}
        options={[
          { value: 'auto', label: 'Automatica', recommended: true },
          { value: 'cubeb', label: 'Cubeb' },
          { value: 'sdl', label: 'SDL' },
        ]}
      />
    </SettingRow>
  </div>
</section>

<style>
  .page {
    padding: var(--s6) var(--s7) var(--s7);
    max-width: 1120px;
  }
  h1 {
    font-size: var(--fs-3xl);
    margin-bottom: var(--s5);
  }
  .sheet {
    background: var(--pannello);
    border-radius: var(--r-panel);
    padding: 0 var(--s5);
  }
</style>
