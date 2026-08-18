<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';

  export let terminalName: string;
  export let initialValue = '';
  export let oncancel: () => void;
  export let onconfirm: (value: string) => void;

  let value = initialValue;
  let input: HTMLTextAreaElement;

  tick().then(() => input?.focus());

  function submit() {
    onconfirm(value.trim());
  }
</script>

<div class="modal-backdrop" role="presentation" on:click|self={oncancel}>
  <form class="modal-card startup-dialog" aria-label="Startup command" on:submit|preventDefault={submit}>
    <header>
      <div class="modal-icon"><Icon name="terminal" size={20} /></div>
      <div><p class="overline">TERMINAL / STARTUP</p><h2>{terminalName}</h2></div>
      <button class="icon-button" type="button" aria-label="Close startup command editor" on:click={oncancel}><Icon name="close" /></button>
    </header>
    <p class="startup-description">Run this command after the terminal starts. Multiple lines are run in order.</p>
    <label for="startup-command">Startup command</label>
    <textarea id="startup-command" bind:this={input} bind:value rows="6" spellcheck="false" placeholder={'cd ~/project\nnpm run dev'}></textarea>
    <div class="modal-actions">
      <button class="button quiet" type="button" on:click={oncancel}>Cancel</button>
      <button class="button primary" type="submit">Save command</button>
    </div>
  </form>
</div>