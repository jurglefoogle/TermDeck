<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';

  type DirectoryEntry = { name: string; path: string; isDirectory: boolean };
  export let initialPath = '';
  export let onclose: () => void;
  export let onopen: (path: string) => void;

  let currentPath = initialPath;
  let entries: DirectoryEntry[] = [];
  let loading = false;
  let error = '';

  async function load(path: string) {
    loading = true;
    error = '';
    try {
      entries = await invoke<DirectoryEntry[]>('list_directory', { path });
      currentPath = path;
    } catch (reason) {
      error = String(reason);
    } finally {
      loading = false;
    }
  }

  function parentPath() {
    const separator = Math.max(currentPath.lastIndexOf('/'), currentPath.lastIndexOf('\\'));
    return separator > 0 ? currentPath.slice(0, separator) : currentPath;
  }

  onMount(() => load(initialPath));
</script>

<aside class="file-browser" aria-label="File browser">
  <header class="file-browser-header">
    <div><Icon name="folder" size={14} /><strong>Files</strong></div>
    <div class="file-browser-actions">
      <button class="icon-button" title="Go to parent folder" aria-label="Go to parent folder" on:click={() => load(parentPath())}><Icon name="arrow-left" size={14} /></button>
      <button class="icon-button" title="Refresh files" aria-label="Refresh files" on:click={() => load(currentPath)}><Icon name="refresh" size={14} /></button>
      <button class="icon-button" title="Close file browser" aria-label="Close file browser" on:click={onclose}><Icon name="close" size={14} /></button>
    </div>
  </header>
  <div class="file-browser-path" title={currentPath}>{currentPath || 'Workspace folder'}</div>
  <div class="file-browser-list">
    {#if loading}<span class="file-browser-empty">Loading...</span>
    {:else if error}<span class="file-browser-empty error">{error}</span>
    {:else if entries.length === 0}<span class="file-browser-empty">Folder is empty</span>
    {:else}
      {#each entries as entry (entry.path)}
        <button class="file-entry" on:dblclick={() => entry.isDirectory ? load(entry.path) : onopen(entry.path)} on:click={() => { if (!entry.isDirectory) onopen(entry.path); }}>
          <Icon name={entry.isDirectory ? 'folder' : 'dock'} size={14} /><span>{entry.name}</span>
        </button>
      {/each}
    {/if}
  </div>
</aside>