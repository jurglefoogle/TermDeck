<script lang="ts">
  import Icon from './Icon.svelte';
  export let onclose: () => void;
  export let onpick: () => void;
</script>

<div class="modal-backdrop" role="presentation" on:click|self={onclose}>
  <section class="modal-card dock-dialog" aria-label="Dock external terminal location">
    <header>
      <div class="modal-icon"><Icon name="dock" size={21} /></div>
      <div><p class="overline">EXTERNAL DOCK</p><h2>Bring a location into TermDeck</h2></div>
      <button class="icon-button" aria-label="Close docking dialog" on:click={onclose}><Icon name="close" /></button>
    </header>
    <div class="dock-drop-illustration">
      <Icon name="folder" size={26} />
      <span>Drop a folder or file anywhere in TermDeck</span>
    </div>
    <p>TermDeck will open a new managed shell at that location and use it as the workspace default for future terminals.</p>
    <div class="constraint-note">
      <strong>Process boundary</strong>
      <span>Windows and Linux cannot transfer an already-running terminal process, its scrollback, or its PTY into another terminal host. Docking recreates the shell at the same location.</span>
    </div>
    <div class="modal-actions">
      <button class="button quiet" on:click={onclose}>Cancel</button>
      <button class="button primary" on:click={onpick}><Icon name="folder" size={15} /> Choose location</button>
    </div>
  </section>
</div>
