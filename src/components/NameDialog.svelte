<script lang="ts">
  import { tick } from 'svelte';

  export let title: string;
  export let initialValue: string;
  export let confirmLabel = 'Save';
  export let oncancel: () => void;
  export let onconfirm: (value: string) => void;

  let value = initialValue;
  let input: HTMLInputElement;

  tick().then(() => {
    input?.focus();
    input?.select();
  });

  function submit() {
    const clean = value.trim();
    if (clean) onconfirm(clean);
  }
</script>

<div class="modal-backdrop" role="presentation" on:click|self={oncancel}>
  <form class="modal-card name-dialog" aria-label={title} on:submit|preventDefault={submit}>
    <p class="overline">TERMDECK / ORGANIZE</p>
    <h2>{title}</h2>
    <label for="name-input">Name</label>
    <input id="name-input" bind:this={input} bind:value maxlength="80" />
    <div class="modal-actions">
      <button class="button quiet" type="button" on:click={oncancel}>Cancel</button>
      <button class="button primary" type="submit" disabled={!value.trim()}>{confirmLabel}</button>
    </div>
  </form>
</div>
