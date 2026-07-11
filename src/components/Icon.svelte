<script lang="ts">
  export let name: 'terminal' | 'plus' | 'folder' | 'edit' | 'close' | 'more' | 'grid' | 'restart' | 'chevron' | 'dock' | 'keyboard' | 'arrow-left' | 'arrow-right' | 'spark' = 'terminal';
  export let size = 16;
</script>

<svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none">
  {#if name === 'terminal'}
    <rect x="3" y="4" width="18" height="16" rx="3" stroke="currentColor" stroke-width="1.7" />
    <path d="m7 9 3 3-3 3m6 0h4" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.7" />
  {:else if name === 'plus'}
    <path d="M12 5v14M5 12h14" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" />
  {:else if name === 'folder'}
    <path d="M3.5 7.5h6l2-2h3l2 2h4v11h-17z" stroke="currentColor" stroke-linejoin="round" stroke-width="1.7" />
  {:else if name === 'edit'}
    <path d="m5 16-.8 3.8L8 19l10.5-10.5-3-3zM13.8 7.2l3 3" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.7" />
  {:else if name === 'close'}
    <path d="m7 7 10 10M17 7 7 17" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" />
  {:else if name === 'more'}
    <circle cx="5" cy="12" r="1.35" fill="currentColor" /><circle cx="12" cy="12" r="1.35" fill="currentColor" /><circle cx="19" cy="12" r="1.35" fill="currentColor" />
  {:else if name === 'grid'}
    <rect x="4" y="4" width="6" height="6" rx="1" stroke="currentColor" stroke-width="1.5" /><rect x="14" y="4" width="6" height="6" rx="1" stroke="currentColor" stroke-width="1.5" /><rect x="4" y="14" width="6" height="6" rx="1" stroke="currentColor" stroke-width="1.5" /><rect x="14" y="14" width="6" height="6" rx="1" stroke="currentColor" stroke-width="1.5" />
  {:else if name === 'restart'}
    <path d="M19 8V4m0 0h-4m4 0-3.1 3.1a7 7 0 1 0 1.7 7.2" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.7" />
  {:else if name === 'chevron'}
    <path d="m9 7 5 5-5 5" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" />
  {:else if name === 'dock'}
    <path d="M4 5h16v11H4zM8 20h8M12 16v4" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.6" /><path d="m8 9 2 2-2 2m4 0h3" stroke="currentColor" stroke-linecap="round" stroke-width="1.5" />
  {:else if name === 'keyboard'}
    <rect x="3" y="6" width="18" height="12" rx="2" stroke="currentColor" stroke-width="1.6" /><path d="M7 10h.01M10.5 10h.01M14 10h.01M17.5 10h.01M7 14h2m2 0h6" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" />
  {:else if name === 'arrow-left'}
    <path d="m14 6-6 6 6 6" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" />
  {:else if name === 'arrow-right'}
    <path d="m10 6 6 6-6 6" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" />
  {:else if name === 'spark'}
    <path d="m12 3 1.5 5.5L19 10l-5.5 1.5L12 17l-1.5-5.5L5 10l5.5-1.5z" stroke="currentColor" stroke-linejoin="round" stroke-width="1.5" />
  {/if}
</svg>

