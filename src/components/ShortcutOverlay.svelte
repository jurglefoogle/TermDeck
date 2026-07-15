<script lang="ts">
  import Icon from './Icon.svelte';
  export let onclose: () => void;

  const groups = [
    { title: 'Terminals', items: [
      ['New terminal', 'Ctrl', 'Shift', 'T'],
      ['Next / previous', 'Ctrl', 'Tab'],
      ['Focus terminal 1–9', 'Ctrl', '1–9'],
      ['Resize active pane split', 'Alt', 'Shift', '← / →'],
      ['Rename focused terminal', 'F2'],
      ['Close focused terminal', 'Ctrl', 'Shift', 'W'],
    ]},
    { title: 'Workspaces', items: [
      ['Switch workspace 1–9', 'Alt', '1–9'],
      ['Previous / next workspace', 'Ctrl', 'Alt', '← / →'],
      ['Move terminal between workspaces', 'Ctrl', 'Shift', '← / →'],
      ['Show shortcuts', 'Ctrl', '/'],
    ]},
  ];
</script>

<div class="modal-backdrop" role="presentation" on:click|self={onclose}>
  <section class="modal-card shortcut-card" aria-label="Keyboard shortcuts">
    <header>
      <div class="modal-icon"><Icon name="keyboard" size={20} /></div>
      <div><p class="overline">NAVIGATION SYSTEM</p><h2>Keyboard shortcuts</h2></div>
      <button class="icon-button" aria-label="Close shortcuts" on:click={onclose}><Icon name="close" /></button>
    </header>
    <div class="shortcut-groups">
      {#each groups as group}
        <div class="shortcut-group">
          <h3>{group.title}</h3>
          {#each group.items as item}
            <div class="shortcut-row">
              <span>{item[0]}</span>
              <div>{#each item.slice(1) as key}<kbd>{key}</kbd>{/each}</div>
            </div>
          {/each}
        </div>
      {/each}
    </div>
  </section>
</div>
