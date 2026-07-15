<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { onMount } from 'svelte';
  import DockDialog from './components/DockDialog.svelte';
  import Icon from './components/Icon.svelte';
  import NameDialog from './components/NameDialog.svelte';
  import SettingsDialog from './components/SettingsDialog.svelte';
  import ShortcutOverlay from './components/ShortcutOverlay.svelte';
  import TerminalPane from './components/TerminalPane.svelte';
  import type { DockPathInfo, EditTarget, EnvironmentInfo, LocatedTerminal, Workspace } from './lib/types';
  import { SETTINGS_STORAGE_KEY, loadSettings, type AppSettings } from './lib/settings';
  import {
    adjustRowSplitRatios,
    ACTIVE_WORKSPACE_KEY,
    DEFAULT_ROW_MIN_SPLIT,
    computeTileLayout,
    createTerminal,
    createWorkspace,
    distributeRows,
    loadWorkspaces,
    normalizeSplitRatiosForRows,
    moveTerminal as moveTerminalConfig,
    STORAGE_KEY,
  } from './lib/workspaces';

  let workspaces: Workspace[] = loadWorkspaces();
  const storedWorkspaceId = localStorage.getItem(ACTIVE_WORKSPACE_KEY);
  let activeWorkspaceId = workspaces.some((workspace) => workspace.id === storedWorkspaceId)
    ? storedWorkspaceId as string
    : workspaces[0].id;
  let homePath = '';
  let platform = 'desktop';
  let shell = 'shell';
  let smokeTest = false;
  let editing: EditTarget | null = null;
  let workspaceMenu: string | null = null;
  let showShortcuts = false;
  let showDockDialog = false;
  let showSettings = false;
  let settings: AppSettings = loadSettings();
  let draggedTerminal: { terminalId: string; sourceWorkspaceId: string } | null = null;
  let dragOverWorkspaceId: string | null = null;
  let externalDropActive = false;
  let externalDropWorkspaceId: string | null = null;
  let toast: { title: string; message: string } | null = null;
  let toastTimer: ReturnType<typeof setTimeout> | null = null;
  let terminalStage: HTMLDivElement | null = null;
  let resizingSplit = false;
  let splitPointerId: number | null = null;
  let activeResizeHandle: { rowIndex: number; handleIndex: number } | null = null;
  const KEYBOARD_SPLIT_STEP = 0.03;

  $: activeWorkspace = workspaces.find((workspace) => workspace.id === activeWorkspaceId) ?? workspaces[0];
  $: activeRows = activeWorkspace ? distributeRows(activeWorkspace.terminals) : [];
  $: normalizedSplitRatios = normalizeSplitRatiosForRows(activeRows, activeWorkspace?.splitRatios);
  $: tileLayout = activeWorkspace ? computeTileLayout(activeWorkspace.terminals, normalizedSplitRatios) : {
    styles: {},
    handles: [],
    rowSplitRatios: [],
  };
  $: tileStyles = tileLayout.styles;
  $: splitHandles = tileLayout.handles;
  $: allTerminals = workspaces.flatMap((workspace): LocatedTerminal[] => workspace.terminals.map((terminal) => ({
    terminal,
    workspaceId: workspace.id,
    workspaceCwd: workspace.cwd,
  })));
  $: localStorage.setItem(STORAGE_KEY, JSON.stringify(workspaces));
  $: localStorage.setItem(ACTIVE_WORKSPACE_KEY, activeWorkspaceId);
  $: localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  $: if (resizingSplit && splitHandles.length === 0) stopSplitResize();

  function notify(title: string, message: string) {
    toast = { title, message };
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { toast = null; }, 4200);
  }

  function updateWorkspace(workspaceId: string, update: (workspace: Workspace) => Workspace) {
    workspaces = workspaces.map((workspace) => workspace.id === workspaceId ? update(workspace) : workspace);
  }

  function addWorkspace() {
    const workspace = createWorkspace(`Workspace ${workspaces.length + 1}`, homePath);
    workspaces = [...workspaces, workspace];
    activeWorkspaceId = workspace.id;
    editing = { kind: 'workspace', workspaceId: workspace.id, value: workspace.name };
  }

  function deleteWorkspace(workspaceId: string) {
    if (workspaces.length === 1) return;
    const workspace = workspaces.find((item) => item.id === workspaceId);
    if (!workspace || !confirm(`Delete “${workspace.name}” and close its ${workspace.terminals.length} terminal(s)?`)) return;
    workspace.terminals.forEach((terminal) => invoke('kill_terminal', { sessionId: terminal.id }).catch(() => undefined));
    const index = workspaces.findIndex((item) => item.id === workspaceId);
    workspaces = workspaces.filter((item) => item.id !== workspaceId);
    if (activeWorkspaceId === workspaceId) activeWorkspaceId = workspaces[Math.min(index, workspaces.length - 1)].id;
    workspaceMenu = null;
  }

  function addTerminal(workspaceId = activeWorkspace.id, cwd?: string, name?: string) {
    const workspace = workspaces.find((item) => item.id === workspaceId);
    if (!workspace) return;
    const terminal = createTerminal(workspace.terminals.length + 1, cwd || workspace.cwd || homePath, name);
    updateWorkspace(workspaceId, (current) => ({
      ...current,
      terminals: [...current.terminals, terminal],
      activeTerminalId: terminal.id,
    }));
    activeWorkspaceId = workspaceId;
  }

  function closeTerminal(workspaceId: string, terminalId: string) {
    invoke('kill_terminal', { sessionId: terminalId }).catch(() => undefined);
    updateWorkspace(workspaceId, (workspace) => {
      const index = workspace.terminals.findIndex((terminal) => terminal.id === terminalId);
      const terminals = workspace.terminals.filter((terminal) => terminal.id !== terminalId);
      return {
        ...workspace,
        terminals,
        activeTerminalId: workspace.activeTerminalId === terminalId
          ? terminals[Math.min(index, terminals.length - 1)]?.id ?? null
          : workspace.activeTerminalId,
      };
    });
  }

  function activateTerminal(workspaceId: string, terminalId: string) {
    activeWorkspaceId = workspaceId;
    updateWorkspace(workspaceId, (workspace) => ({ ...workspace, activeTerminalId: terminalId }));
  }

  function updateTerminalCwd(workspaceId: string, terminalId: string, cwd: string) {
    updateWorkspace(workspaceId, (workspace) => ({
      ...workspace,
      terminals: workspace.terminals.map((terminal) => (
        terminal.id === terminalId && terminal.cwd !== cwd ? { ...terminal, cwd } : terminal
      )),
    }));
  }

  function cycleTerminal(direction: number) {
    if (!activeWorkspace?.terminals.length) return;
    const index = activeWorkspace.terminals.findIndex((terminal) => terminal.id === activeWorkspace.activeTerminalId);
    const next = (index + direction + activeWorkspace.terminals.length) % activeWorkspace.terminals.length;
    activateTerminal(activeWorkspace.id, activeWorkspace.terminals[next].id);
  }

  function cycleWorkspace(direction: number) {
    const index = workspaces.findIndex((workspace) => workspace.id === activeWorkspaceId);
    const next = (index + direction + workspaces.length) % workspaces.length;
    activeWorkspaceId = workspaces[next].id;
  }

  function moveTerminal(terminalId: string, sourceWorkspaceId: string, targetWorkspaceId: string, selectTarget = true) {
    if (sourceWorkspaceId === targetWorkspaceId) return;
    workspaces = moveTerminalConfig(workspaces, terminalId, sourceWorkspaceId, targetWorkspaceId);
    if (selectTarget) activeWorkspaceId = targetWorkspaceId;
    const target = workspaces.find((workspace) => workspace.id === targetWorkspaceId);
    notify('Terminal moved', `Docked in ${target?.name ?? 'workspace'}. The live process stayed attached.`);
  }

  function setWorkspaceSplitRatios(workspaceId: string, splitRatios: number[][]) {
    updateWorkspace(workspaceId, (workspace) => ({ ...workspace, splitRatios }));
  }

  function updateSplitRatioFromPointer(clientX: number) {
    if (!terminalStage || !activeResizeHandle) return;
    const handle = activeResizeHandle;
    const bounds = terminalStage.getBoundingClientRect();
    if (bounds.width <= 0) return;
    const rawBoundary = (clientX - bounds.left) / bounds.width;
    const nextSplitRatios = adjustRowSplitRatios(
      normalizedSplitRatios,
      handle.rowIndex,
      handle.handleIndex,
      rawBoundary,
      DEFAULT_ROW_MIN_SPLIT,
    );
    if (!nextSplitRatios) return;
    setWorkspaceSplitRatios(activeWorkspace.id, nextSplitRatios);
  }

  function resizeActiveSplit(direction: -1 | 1): boolean {
    const activeTerminalId = activeWorkspace.activeTerminalId;
    if (!activeTerminalId) return false;
    const rowIndex = activeRows.findIndex((row) => row.some((terminal) => terminal.id === activeTerminalId));
    if (rowIndex < 0) return false;
    const row = activeRows[rowIndex];
    if (row.length < 2) return false;
    const columnIndex = row.findIndex((terminal) => terminal.id === activeTerminalId);
    if (columnIndex < 0) return false;

    const handleIndex = direction > 0
      ? (columnIndex < row.length - 1 ? columnIndex : columnIndex - 1)
      : (columnIndex > 0 ? columnIndex - 1 : 0);
    const rowRatios = normalizedSplitRatios[rowIndex];
    if (!rowRatios) return false;
    const boundary = rowRatios.slice(0, handleIndex + 1).reduce((total, value) => total + value, 0);
    const nextSplitRatios = adjustRowSplitRatios(
      normalizedSplitRatios,
      rowIndex,
      handleIndex,
      boundary + (direction * KEYBOARD_SPLIT_STEP),
      DEFAULT_ROW_MIN_SPLIT,
    );
    if (!nextSplitRatios) return false;
    setWorkspaceSplitRatios(activeWorkspace.id, nextSplitRatios);
    return true;
  }

  function handleSplitPointerMove(event: PointerEvent) {
    if (!resizingSplit || splitPointerId !== event.pointerId) return;
    updateSplitRatioFromPointer(event.clientX);
  }

  function stopSplitResize(pointerId?: number) {
    if (!resizingSplit) return;
    if (pointerId !== undefined && splitPointerId !== pointerId) return;
    resizingSplit = false;
    splitPointerId = null;
    activeResizeHandle = null;
    window.removeEventListener('pointermove', handleSplitPointerMove);
    window.removeEventListener('pointerup', handleSplitPointerUp);
    window.removeEventListener('pointercancel', handleSplitPointerCancel);
  }

  function handleSplitPointerUp(event: PointerEvent) {
    stopSplitResize(event.pointerId);
  }

  function handleSplitPointerCancel(event: PointerEvent) {
    stopSplitResize(event.pointerId);
  }

  function startSplitResize(event: PointerEvent, rowIndex: number, handleIndex: number) {
    if (!splitHandles.some((handle) => handle.rowIndex === rowIndex && handle.handleIndex === handleIndex)) return;
    event.preventDefault();
    event.stopPropagation();
    splitPointerId = event.pointerId;
    activeResizeHandle = { rowIndex, handleIndex };
    resizingSplit = true;
    window.addEventListener('pointermove', handleSplitPointerMove);
    window.addEventListener('pointerup', handleSplitPointerUp);
    window.addEventListener('pointercancel', handleSplitPointerCancel);
    updateSplitRatioFromPointer(event.clientX);
  }

  function moveActiveTerminal(direction: number) {
    if (!activeWorkspace?.activeTerminalId || workspaces.length < 2) return;
    const sourceIndex = workspaces.findIndex((workspace) => workspace.id === activeWorkspace.id);
    const targetIndex = (sourceIndex + direction + workspaces.length) % workspaces.length;
    moveTerminal(activeWorkspace.activeTerminalId, activeWorkspace.id, workspaces[targetIndex].id);
  }

  function startTerminalDrag(event: DragEvent, terminalId: string, sourceWorkspaceId: string) {
    draggedTerminal = { terminalId, sourceWorkspaceId };
    event.dataTransfer?.setData('application/x-termdeck-terminal', JSON.stringify(draggedTerminal));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
  }

  function dropTerminalOnWorkspace(event: DragEvent, targetWorkspaceId: string) {
    event.preventDefault();
    event.stopPropagation();
    const payload = draggedTerminal;
    if (payload) moveTerminal(payload.terminalId, payload.sourceWorkspaceId, targetWorkspaceId);
    draggedTerminal = null;
    dragOverWorkspaceId = null;
  }

  function saveName(value: string) {
    if (!editing) return;
    if (editing.kind === 'workspace') {
      updateWorkspace(editing.workspaceId, (workspace) => ({ ...workspace, name: value }));
    } else {
      const terminalId = editing.terminalId;
      updateWorkspace(editing.workspaceId, (workspace) => ({
        ...workspace,
        terminals: workspace.terminals.map((terminal) => terminal.id === terminalId ? { ...terminal, name: value } : terminal),
      }));
    }
    editing = null;
  }

  async function chooseWorkspaceFolder() {
    const selected = await open({ directory: true, multiple: false, defaultPath: activeWorkspace.cwd || homePath });
    if (typeof selected !== 'string') return;
    updateWorkspace(activeWorkspace.id, (workspace) => ({ ...workspace, cwd: selected }));
    notify('Workspace location updated', 'New terminals will open in the selected folder.');
  }

  async function dockPath(path: string, targetWorkspaceId = activeWorkspace.id) {
    try {
      const info = await invoke<DockPathInfo>('normalize_dock_path', { path });
      updateWorkspace(targetWorkspaceId, (workspace) => ({ ...workspace, cwd: info.directory }));
      addTerminal(targetWorkspaceId, info.directory, info.suggestedName);
      showDockDialog = false;
      notify('Location docked', `Opened a managed shell at ${info.directory}`);
    } catch (error) {
      notify('Unable to dock location', String(error));
    }
  }

  async function pickDockLocation() {
    const selected = await open({ directory: true, multiple: false, defaultPath: activeWorkspace.cwd || homePath });
    if (typeof selected === 'string') await dockPath(selected);
  }

  function workspaceAtPoint(position: { x: number; y: number } | undefined): string {
    if (!position) return activeWorkspace.id;
    const scale = window.devicePixelRatio || 1;
    const candidates = [
      document.elementFromPoint(position.x / scale, position.y / scale),
      document.elementFromPoint(position.x, position.y),
    ];
    for (const candidate of candidates) {
      const workspaceId = candidate?.closest<HTMLElement>('[data-workspace-id]')?.dataset.workspaceId;
      if (workspaceId) return workspaceId;
    }
    return activeWorkspace.id;
  }

  function renameActiveTerminal() {
    const terminal = activeWorkspace.terminals.find((item) => item.id === activeWorkspace.activeTerminalId);
    if (terminal) editing = { kind: 'terminal', workspaceId: activeWorkspace.id, terminalId: terminal.id, value: terminal.name };
  }

  function handleKeyboard(event: KeyboardEvent) {
    if (editing || showDockDialog || showSettings) {
      if (event.key === 'Escape') { editing = null; showDockDialog = false; showSettings = false; }
      return;
    }
    if (showShortcuts && event.key === 'Escape') { showShortcuts = false; return; }

    if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 't') {
      event.preventDefault(); addTerminal();
    } else if (event.ctrlKey && event.key === 'Tab') {
      event.preventDefault(); cycleTerminal(event.shiftKey ? -1 : 1);
    } else if (event.altKey && !event.ctrlKey && /^[1-9]$/.test(event.key)) {
      const workspace = workspaces[Number(event.key) - 1];
      if (workspace) { event.preventDefault(); activeWorkspaceId = workspace.id; }
    } else if (event.ctrlKey && !event.altKey && !event.shiftKey && /^[1-9]$/.test(event.key)) {
      const terminal = activeWorkspace.terminals[Number(event.key) - 1];
      if (terminal) { event.preventDefault(); activateTerminal(activeWorkspace.id, terminal.id); }
    } else if (event.ctrlKey && event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
      event.preventDefault(); cycleWorkspace(event.key === 'ArrowLeft' ? -1 : 1);
    } else if (!event.ctrlKey && event.altKey && event.shiftKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
      if (resizeActiveSplit(event.key === 'ArrowLeft' ? -1 : 1)) event.preventDefault();
    } else if (event.ctrlKey && event.shiftKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
      event.preventDefault(); moveActiveTerminal(event.key === 'ArrowLeft' ? -1 : 1);
    } else if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'w') {
      event.preventDefault();
      if (activeWorkspace.activeTerminalId) closeTerminal(activeWorkspace.id, activeWorkspace.activeTerminalId);
    } else if (event.key === 'F2') {
      event.preventDefault(); renameActiveTerminal();
    } else if (event.ctrlKey && event.key === '/') {
      event.preventDefault(); showShortcuts = !showShortcuts;
    }
  }

  onMount(() => {
    let unlistenDrop: (() => void) | undefined;
    invoke<EnvironmentInfo>('get_environment').then((environment) => {
      homePath = environment.home;
      platform = environment.platform;
      shell = environment.shell;
      smokeTest = environment.smokeTest;
      workspaces = workspaces.map((workspace) => {
        const cwd = workspace.cwd || environment.home;
        return {
          ...workspace,
          cwd,
          terminals: workspace.terminals.map((terminal) => ({ ...terminal, cwd: terminal.cwd || cwd })),
        };
      });
      if (environment.smokeTest) {
        let attempts = 0;
        const verifyDesktop = async () => {
          attempts += 1;
          try {
            const running = await invoke<number>('running_terminal_count');
            if (running > 0) {
              await invoke('complete_smoke_test', { success: true, message: `${running} PTY session(s) running` });
              return;
            }
          } catch {
            // Retry while the first Svelte terminal component finishes mounting.
          }
          if (attempts < 20) setTimeout(verifyDesktop, 250);
          else await invoke('complete_smoke_test', { success: false, message: 'No PTY session started' });
        };
        setTimeout(verifyDesktop, 250);
      }
    }).catch(() => undefined);

    window.addEventListener('keydown', handleKeyboard, true);
    if ('__TAURI_INTERNALS__' in window) {
      getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === 'over') {
          externalDropActive = true;
          externalDropWorkspaceId = workspaceAtPoint(event.payload.position);
        } else if (event.payload.type === 'drop') {
          const target = externalDropWorkspaceId || workspaceAtPoint(event.payload.position);
          externalDropActive = false;
          externalDropWorkspaceId = null;
          const path = event.payload.paths[0];
          if (path) dockPath(path, target);
        } else {
          externalDropActive = false;
          externalDropWorkspaceId = null;
        }
      }).then((unlisten) => { unlistenDrop = unlisten; });
    }

    return () => {
      stopSplitResize();
      window.removeEventListener('keydown', handleKeyboard, true);
      unlistenDrop?.();
      if (toastTimer) clearTimeout(toastTimer);
    };
  });
</script>

<div class="aevum-frame" role="presentation" on:click={() => { workspaceMenu = null; }}>
  <div class="aevum-shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-symbol"><Icon name="terminal" size={20} /></div>
        <div><strong>TermDeck</strong><span>AEVUM WORKSPACE CONSOLE</span></div>
      </div>

      <div class="sidebar-heading">
        <span>WORKSPACES</span>
        <button class="icon-button" aria-label="New workspace" title="New workspace" on:click={addWorkspace}><Icon name="plus" /></button>
      </div>

      <nav class="workspace-list" aria-label="Workspaces">
        {#each workspaces as workspace, index (workspace.id)}
          <div
            role="group"
            class:active={workspace.id === activeWorkspace.id}
            class:drag-over={dragOverWorkspaceId === workspace.id || externalDropWorkspaceId === workspace.id}
            class="workspace-row"
            data-workspace-id={workspace.id}
            on:dragover={(event) => { event.preventDefault(); dragOverWorkspaceId = workspace.id; }}
            on:dragleave={() => { if (dragOverWorkspaceId === workspace.id) dragOverWorkspaceId = null; }}
            on:drop={(event) => dropTerminalOnWorkspace(event, workspace.id)}
          >
            <button class="workspace-button" on:click={() => { activeWorkspaceId = workspace.id; }}>
              <span class="workspace-number">{String(index + 1).padStart(2, '0')}</span>
              <span class="workspace-copy"><strong>{workspace.name}</strong><small>{workspace.terminals.length} live terminal{workspace.terminals.length === 1 ? '' : 's'}</small></span>
            </button>
            <button class="workspace-more" aria-label={`${workspace.name} actions`} on:click|stopPropagation={() => { workspaceMenu = workspaceMenu === workspace.id ? null : workspace.id; }}><Icon name="more" /></button>
            {#if workspaceMenu === workspace.id}
              <div class="workspace-menu">
                <button on:click={() => { editing = { kind: 'workspace', workspaceId: workspace.id, value: workspace.name }; workspaceMenu = null; }}><Icon name="edit" size={14} /> Rename</button>
                <button class="danger-text" disabled={workspaces.length === 1} on:click={() => deleteWorkspace(workspace.id)}><Icon name="close" size={14} /> Delete</button>
              </div>
            {/if}
          </div>
        {/each}
      </nav>

      <button class="new-workspace" on:click={addWorkspace}><Icon name="plus" /> New workspace</button>
      <div class="sidebar-tools">
        <button on:click={() => { showDockDialog = true; }}><Icon name="dock" /><span><strong>Dock external</strong><small>Drop a location</small></span></button>
        <button on:click={() => { showShortcuts = true; }}><Icon name="keyboard" /><span><strong>Shortcuts</strong><small>Ctrl + /</small></span></button>
        <button on:click={() => { showSettings = true; }}><Icon name="settings" /><span><strong>Settings</strong><small>Terminal preferences</small></span></button>
      </div>
      <div class="sidebar-footer">
        <span><i></i> RUST PTY</span><span>{platform.toUpperCase()} · {shell.split(/[\\/]/).pop()}</span>
      </div>
    </aside>

    <main class="main-area">
      <header class="main-header">
        <div class="workspace-title">
          <div class="workspace-icon"><Icon name="grid" size={18} /></div>
          <div>
            <div class="title-line"><h1>{activeWorkspace.name}</h1><button class="bare-icon" title="Rename workspace" on:click={() => { editing = { kind: 'workspace', workspaceId: activeWorkspace.id, value: activeWorkspace.name }; }}><Icon name="edit" size={13} /></button></div>
            <button class="path-button" title="Default folder for new terminals" on:click={chooseWorkspaceFolder}><Icon name="folder" size={13} /><span>{activeWorkspace.cwd || homePath || 'Home folder'}</span><Icon name="chevron" size={12} /></button>
          </div>
        </div>
        <div class="header-actions">
          <button class="button quiet dock-button" on:click={() => { showDockDialog = true; }}><Icon name="dock" size={15} /> Dock external</button>
          <button class="button primary" on:click={() => addTerminal()}><Icon name="plus" size={16} /> New terminal <kbd>Ctrl ⇧ T</kbd></button>
        </div>
      </header>

      <div class="tab-strip" role="tablist" aria-label="Terminal tabs">
        {#each activeWorkspace.terminals as terminal, index (terminal.id)}
          <button
            class:active={terminal.id === activeWorkspace.activeTerminalId}
            class="terminal-tab"
            role="tab"
            aria-selected={terminal.id === activeWorkspace.activeTerminalId}
            draggable="true"
            on:dragstart={(event) => startTerminalDrag(event, terminal.id, activeWorkspace.id)}
            on:click={() => activateTerminal(activeWorkspace.id, terminal.id)}
            on:dblclick={() => { editing = { kind: 'terminal', workspaceId: activeWorkspace.id, terminalId: terminal.id, value: terminal.name }; }}
          ><span>{String(index + 1).padStart(2, '0')}</span>{terminal.name}<i></i></button>
        {/each}
        <button class="tab-add" aria-label="New terminal" on:click={() => addTerminal()}><Icon name="plus" size={15} /></button>
      </div>

      <div class="terminal-stage" class:is-resizing={resizingSplit} bind:this={terminalStage}>
        {#if activeWorkspace.terminals.length === 0}
          <div class="empty-state"><div class="empty-icon"><Icon name="terminal" size={28} /></div><p class="overline">AVAILABLE WORKSPACE</p><h2>Ready for a terminal</h2><p>New shells open at <strong>{activeWorkspace.cwd || homePath}</strong> and tile automatically.</p><button class="button primary" on:click={() => addTerminal()}><Icon name="plus" /> New terminal</button></div>
        {/if}

        {#each allTerminals as located (located.terminal.id)}
          <TerminalPane
            terminal={located.terminal}
            {smokeTest}
            visible={located.workspaceId === activeWorkspace.id}
            active={located.workspaceId === activeWorkspace.id && located.terminal.id === activeWorkspace.activeTerminalId}
            positionStyle={tileStyles[located.terminal.id] ?? ''}
            onactivate={() => activateTerminal(located.workspaceId, located.terminal.id)}
            onclose={() => closeTerminal(located.workspaceId, located.terminal.id)}
            onrename={() => { editing = { kind: 'terminal', workspaceId: located.workspaceId, terminalId: located.terminal.id, value: located.terminal.name }; }}
            ondragstart={(event) => startTerminalDrag(event, located.terminal.id, located.workspaceId)}
            oncwdchange={(cwd) => updateTerminalCwd(located.workspaceId, located.terminal.id, cwd)}
          />
        {/each}

        {#each splitHandles as handle (`${handle.rowIndex}-${handle.handleIndex}`)}
          <div
            class="terminal-split-handle"
            role="separator"
            aria-label={`Resize terminal panes on row ${handle.rowIndex + 1}`}
            aria-orientation="vertical"
            style={`left: calc(${handle.leftPercent}% - 3px); top: calc(${handle.topPercent}% + 8px); height: calc(${handle.heightPercent}% - 16px)`}
            on:pointerdown={(event) => startSplitResize(event, handle.rowIndex, handle.handleIndex)}
          >
            <span></span>
          </div>
        {/each}

        {#if externalDropActive}
          <div class="external-drop-overlay">
            <div><Icon name="dock" size={28} /><strong>Dock location</strong><span>Open in {workspaces.find((workspace) => workspace.id === (externalDropWorkspaceId || activeWorkspace.id))?.name}</span></div>
          </div>
        {/if}
      </div>

      <footer class="status-bar"><span><i></i> Native PTY connected</span><span>Ctrl+Tab terminals · Alt+Shift+←/→ resize · Ctrl+/ shortcuts</span><span>AUTO TILE <Icon name="grid" size={12} /></span></footer>
    </main>
  </div>
</div>

{#if editing}
  <NameDialog title={editing.kind === 'workspace' ? 'Name this workspace' : 'Name this terminal'} initialValue={editing.value} confirmLabel="Save name" oncancel={() => { editing = null; }} onconfirm={saveName} />
{/if}
{#if showShortcuts}<ShortcutOverlay onclose={() => { showShortcuts = false; }} />{/if}
{#if showDockDialog}<DockDialog onclose={() => { showDockDialog = false; }} onpick={pickDockLocation} />{/if}
{#if showSettings}<SettingsDialog {settings} onchange={(next) => { settings = next; }} onclose={() => { showSettings = false; }} />{/if}
{#if toast}
  <div class="toast"><div class="toast-icon"><Icon name="spark" /></div><div><strong>{toast.title}</strong><span>{toast.message}</span></div><button aria-label="Dismiss notification" on:click={() => { toast = null; }}><Icon name="close" size={14} /></button></div>
{/if}
