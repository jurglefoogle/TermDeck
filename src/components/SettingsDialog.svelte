<script lang="ts">
  import type { AppSettings } from '../lib/settings';
  import Icon from './Icon.svelte';

  export let settings: AppSettings;
  export let onclose: () => void;
  export let onchange: (settings: AppSettings) => void;

  function toggle(setting: keyof AppSettings) {
    onchange({ ...settings, [setting]: !settings[setting] });
  }
</script>

<div class="modal-backdrop" role="presentation" on:click|self={onclose}>
  <section class="modal-card settings-card" aria-label="Settings">
    <header>
      <div class="modal-icon"><Icon name="settings" size={20} /></div>
      <div><p class="overline">APPLICATION PREFERENCES</p><h2>Settings</h2></div>
      <button class="icon-button" aria-label="Close settings" on:click={onclose}><Icon name="close" /></button>
    </header>

    <div class="settings-group">
      <div class="settings-group-copy">
        <strong>Terminal retention</strong>
        <span>Choose whether future retention support may save data locally on this device.</span>
      </div>
      <div class="settings-option">
        <div><strong>Command history</strong><span>Save native shell history between TermDeck launches.</span></div>
        <button
          class:enabled={settings.retainCommandHistory}
          class="settings-toggle"
          type="button"
          role="switch"
          aria-checked={settings.retainCommandHistory}
          aria-label="Retain command history"
          on:click={() => toggle('retainCommandHistory')}
        ><i></i></button>
      </div>
      <div class="settings-option">
        <div><strong>Terminal scrollback</strong><span>Save terminal output locally between TermDeck launches.</span></div>
        <button
          class:enabled={settings.retainScrollback}
          class="settings-toggle"
          type="button"
          role="switch"
          aria-checked={settings.retainScrollback}
          aria-label="Retain terminal scrollback"
          on:click={() => toggle('retainScrollback')}
        ><i></i></button>
      </div>
    </div>
    <p class="settings-note">Retention is not captured in this release. These saved preferences will be used when command history and scrollback restoration are added.</p>
  </section>
</div>

