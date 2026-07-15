<script lang="ts">
	import type { RunDirectoryScan } from '$lib/api';
	import { directoryPath } from '$lib/runsView';

	let {
		open,
		busy,
		completedDirectory,
		failedDirectory,
		close,
		reveal
	}: {
		open: boolean;
		busy: boolean;
		completedDirectory?: RunDirectoryScan;
		failedDirectory?: RunDirectoryScan;
		close: () => void;
		reveal: (kind: RunDirectoryScan['kind']) => void;
	} = $props();
</script>

{#if open}
	<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4">
		<button
			type="button"
			aria-label="Close clips folder chooser"
			class="absolute inset-0 cursor-default"
			onclick={close}
		></button>
		<dialog
			open
			aria-label="Choose clips folder"
			class="obs-dialog relative z-10 m-0 w-full max-w-sm overflow-hidden rounded p-0"
		>
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading text-lg font-semibold">Open clips folder</h2>
				<p class="obs-dim mt-1 font-mono text-xs">Choose which configured output folder to reveal.</p>
			</header>
			<div class="grid gap-3 p-4">
				<button
					type="button"
					class="obs-list-button grid gap-1 rounded px-3 py-3 text-left"
					disabled={busy || !completedDirectory}
					onclick={() => reveal('completed')}
				>
					<span class="obs-list-title text-sm font-semibold">Completed clips</span>
					<span class="obs-list-detail font-mono text-xs wrap-break-word">{directoryPath(completedDirectory)}</span>
				</button>
				<button
					type="button"
					class="obs-list-button grid gap-1 rounded px-3 py-3 text-left"
					disabled={busy || !failedDirectory}
					onclick={() => reveal('failed')}
				>
					<span class="obs-list-title text-sm font-semibold">Failed clips</span>
					<span class="obs-list-detail font-mono text-xs wrap-break-word">{directoryPath(failedDirectory)}</span>
				</button>
				<div class="flex justify-end">
					<button type="button" class="obs-text-button px-2 py-1 font-mono text-xs" disabled={busy} onclick={close}
						>close</button
					>
				</div>
			</div>
		</dialog>
	</div>
{/if}
