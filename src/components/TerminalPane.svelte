<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { onMount, tick } from 'svelte';
  import { FitAddon } from '@xterm/addon-fit';
  import { Terminal } from '@xterm/xterm';
  import '@xterm/xterm/css/xterm.css';
  import type { PtyEvent, TerminalInfo, TerminalSession } from '../lib/types';
  import Icon from './Icon.svelte';

  export let terminal: TerminalSession;
  export let smokeTest = false;
  export let visible: boolean;
  export let active: boolean;
  export let positionStyle: string;
  export let onactivate: () => void;
  export let onclose: () => void;
  export let onrename: () => void;
  export let ondragstart: (event: DragEvent) => void;

  let host: HTMLDivElement;
  let xterm: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let generation = 0;
  let status: 'starting' | 'running' | 'exited' | 'preview' = 'starting';
  let shellName = 'shell';
  let pendingEvents: PtyEvent[] = [];

  const theme = {
    background: '#111216', foreground: '#d8d8de', cursor: '#8ea4ff', cursorAccent: '#111216',
    selectionBackground: '#5b7cf64d', black: '#15161a', red: '#e78284', green: '#a6d189',
    yellow: '#e5c890', blue: '#8caaee', magenta: '#ca9ee6', cyan: '#81c8be', white: '#d8d8de',
    brightBlack: '#626472', brightRed: '#ef9f9f', brightGreen: '#bfe3a6', brightYellow: '#f1d7a5',
    brightBlue: '#a8baff', brightMagenta: '#dab4f2', brightCyan: '#9bd8d0', brightWhite: '#f0f0f4',
  };

  function decodeBase64(value: string): Uint8Array {
    const binary = atob(value);
    return Uint8Array.from(binary, (character) => character.charCodeAt(0));
  }

  function handlePtyEvent(event: PtyEvent) {
    if (event.sessionId !== terminal.id) return;
    if (generation === 0) {
      pendingEvents.push(event);
      return;
    }
    if (event.generation !== generation || !xterm) return;
    if (event.kind === 'output' && event.data) {
      xterm.write(decodeBase64(event.data));
    } else if (event.kind === 'exit') {
      status = 'exited';
      xterm.writeln(`\r\n\x1b[90m[process exited${event.exitCode !== undefined ? ` with code ${event.exitCode}` : ''}]\x1b[0m`);
    } else if (event.kind === 'error') {
      xterm.writeln(`\r\n\x1b[31m[PTY error: ${event.message ?? 'unknown error'}]\x1b[0m`);
    }
  }

  async function fitAndResize() {
    if (!visible || !host || host.clientWidth < 20 || host.clientHeight < 20 || !xterm || !fitAddon) return;
    try {
      fitAddon.fit();
      if (generation > 0) {
        await invoke('resize_terminal', { sessionId: terminal.id, cols: xterm.cols, rows: xterm.rows });
      }
    } catch {
      // Resizing can race a workspace switch or process exit.
    }
  }

  async function boot() {
    if (!xterm) return;
    status = 'starting';
    generation = 0;
    pendingEvents = [];
    await tick();
    await fitAndResize();
    try {
      const info = await invoke<TerminalInfo>('spawn_terminal', {
        sessionId: terminal.id,
        cwd: terminal.cwd,
        cols: xterm.cols,
        rows: xterm.rows,
      });
      generation = info.generation;
      shellName = info.shell.split(/[\\/]/).pop() || info.shell;
      status = 'running';
      const queued = pendingEvents;
      pendingEvents = [];
      queued.forEach(handlePtyEvent);
      await fitAndResize();
      if (visible && active) xterm.focus();
    } catch (error) {
      const isDesktop = '__TAURI_INTERNALS__' in window;
      status = isDesktop ? 'exited' : 'preview';
      if (isDesktop) xterm.writeln(`\r\n\x1b[31mUnable to start shell: ${String(error)}\x1b[0m`);
      if (smokeTest) {
        invoke('complete_smoke_test', { success: false, message: `PTY startup failed: ${String(error)}` }).catch(() => undefined);
      }
    }
  }

  async function restart() {
    try { await invoke('kill_terminal', { sessionId: terminal.id }); } catch { /* already stopped */ }
    xterm?.reset();
    await boot();
  }

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    let destroyed = false;
    xterm = new Terminal({
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: '"Cascadia Code", "JetBrains Mono", Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.18,
      scrollback: 6000,
      theme,
    });
    fitAddon = new FitAddon();
    xterm.loadAddon(fitAddon);
    xterm.open(host);

    const input = xterm.onData((data) => {
      if (generation > 0) invoke('write_terminal', { sessionId: terminal.id, data }).catch(() => undefined);
    });
    const observer = new ResizeObserver(() => fitAndResize());
    observer.observe(host);

    (async () => {
      try {
        unlisten = await listen<PtyEvent>('pty-event', (event) => handlePtyEvent(event.payload));
      } catch (error) {
        xterm?.writeln(`\r\n\x1b[31mUnable to subscribe to PTY output: ${String(error)}\x1b[0m`);
      }
      if (!destroyed) await boot();
    })();

    return () => {
      destroyed = true;
      observer.disconnect();
      input.dispose();
      unlisten?.();
      xterm?.dispose();
      xterm = null;
      fitAddon = null;
    };
  });

  $: if (visible && positionStyle && fitAddon) {
    tick().then(() => fitAndResize());
  }
  $: if (visible && active && xterm) {
    tick().then(() => xterm?.focus());
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section
  class:active
  class:visible
  class="terminal-pane"
  style={visible ? positionStyle : 'display: none'}
  aria-label={`${terminal.name} terminal`}
  on:mousedown={onactivate}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header class="terminal-pane-header" draggable="true" on:dragstart={ondragstart}>
    <div class="terminal-title-wrap">
      <span class="status-dot {status}"></span>
      <Icon name="terminal" size={14} />
      <button class="terminal-title" title="Rename terminal" on:click|stopPropagation={onrename}>{terminal.name}</button>
      <span class="shell-label">{shellName}</span>
      <span class="cwd-label" title={terminal.cwd}>{terminal.cwd}</span>
    </div>
    <div class="terminal-actions">
      {#if status === 'exited'}
        <button class="icon-button" title="Restart terminal" on:click|stopPropagation={restart}><Icon name="restart" size={14} /></button>
      {/if}
      <button class="icon-button" title="Rename terminal" on:click|stopPropagation={onrename}><Icon name="edit" size={13} /></button>
      <button class="icon-button danger" title="Close terminal" on:click|stopPropagation={onclose}><Icon name="close" size={14} /></button>
    </div>
  </header>
  <div class="terminal-host">
    <div class="terminal-mount" bind:this={host}></div>
  </div>
  {#if status === 'preview'}
    <div class="preview-message"><Icon name="terminal" size={22} /><span>Native PTY active in the desktop build</span></div>
  {/if}
</section>
