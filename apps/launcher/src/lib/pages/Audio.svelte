<script lang="ts">
  import { tr } from '../i18n.svelte';
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
    <SettingRow label={tr('Volume')} description={tr('Also adjustable in game from the L3 + R3 menu.')}>
      <Slider label={tr('Volume')} bind:value={s.audio.volume} min={0} max={100} step={5} format={(v) => `${v}%`} />
    </SettingRow>
    <SettingRow label={tr('Mute when the window is not active')} description={tr('Handy if you play in a window.')}>
      <Toggle label={tr('Mute in the background')} bind:checked={s.audio.muteInBackground} />
    </SettingRow>
    <SettingRow label={tr('Audio output')} description={tr('Automatic picks the most stable one for your system.')}>
      <Segmented
        label={tr('Audio output')}
        bind:value={s.audio.output}
        options={[
          { value: 'auto', label: tr('Automatic'), recommended: true },
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
