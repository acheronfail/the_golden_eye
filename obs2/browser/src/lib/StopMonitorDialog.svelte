<script lang="ts">
	let {
		open,
		busy = false,
		modeTitle = 'single segment run',
		close,
		discard,
		save
	}: {
		open: boolean;
		busy?: boolean;
		modeTitle?: string;
		close: () => void;
		discard: () => void;
		save: () => void;
	} = $props();
</script>

{#if open}
	<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4">
		<button type="button" aria-label="Cancel stop monitor" class="absolute inset-0 cursor-default" onclick={close}></button>
		<dialog open aria-label="Stop monitor confirmation" class="obs-dialog relative z-10 m-0 w-full max-w-sm overflow-hidden rounded p-0">
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading text-lg font-semibold">Stop monitoring?</h2>
				<p class="obs-dim mt-1 font-mono text-xs">This will end the active {modeTitle} session.</p>
			</header>
			<div class="grid gap-3 p-4">
				<p class="obs-muted text-sm">Do you want to save a clip of this single segment recording?</p>
				<div class="flex flex-wrap justify-end gap-2">
					<button type="button" class="obs-text-button px-2 py-1 font-mono text-xs" disabled={busy} onclick={close}>cancel</button>
					<button type="button" class="obs-button obs-button-danger px-3 py-2 font-mono text-xs" disabled={busy} onclick={discard}>
						{busy ? 'stopping...' : 'discard recording'}
					</button>
					<button type="button" class="obs-button px-3 py-2 font-mono text-xs" disabled={busy} onclick={save}>
						{busy ? 'stopping...' : 'save recording'}
					</button>
				</div>
			</div>
		</dialog>
	</div>
{/if}
