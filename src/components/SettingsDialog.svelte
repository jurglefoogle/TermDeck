<script lang="ts">
  import type { AppSettings } from '../lib/settings';
  import {
    MAX_SCROLLBACK_LINES,
    MIN_SCROLLBACK_LINES,
    clampScrollbackLines,
  } from '../lib/terminal-retention';
  import Icon from './Icon.svelte';

  export let settings: AppSettings;
  export let onclose: () => void;
  export let onchange: (settings: AppSettings) => void;

  function toggle(setting: keyof AppSettings) {
    if (setting === 'scrollbackLines') return;
    onchange({ ...settings, [setting]: !settings[setting] });
  }

  function updateScrollbackLines(value: string) {
    onchange({ ...settings, scrollbackLines: clampScrollbackLines(Number(value)) });
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
        <span>Choose whether TermDeck saves terminal data locally on this device.</span>
      </div>
      <div class="settings-option">
        <div><strong>Command history</strong><span>Restore commands entered in this terminal with Up Arrow.</span></div>
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
      <label class="settings-number">
        <span><strong>Scrollback line limit</strong><small>{MIN_SCROLLBACK_LINES.toLocaleString()}–{MAX_SCROLLBACK_LINES.toLocaleString()} lines per terminal</small></span>
        <input
          type="number"
          min={MIN_SCROLLBACK_LINES}
          max={MAX_SCROLLBACK_LINES}
          step="100"
          value={settings.scrollbackLines}
          disabled={!settings.retainScrollback}
          aria-label="Scrollback line limit"
          on:change={(event) => updateScrollbackLines(event.currentTarget.value)}
        />
      </label>
    </div>
    <p class="settings-note">Retained data stays in this browser profile. Turning an option off removes its saved data.</p>
  </section>
</div>
