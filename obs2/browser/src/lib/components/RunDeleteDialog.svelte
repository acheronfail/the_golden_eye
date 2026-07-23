<script lang="ts">
	import type { RunClip } from '$lib/api';
	let {
		run,
		busy,
		error,
		onCancel,
		onDeleteVideo,
		onDeleteAll
	}: {
		run: RunClip | null;
		busy: boolean;
		error: string | null;
		onCancel: () => void;
		onDeleteVideo: () => void;
		onDeleteAll: () => void;
	} = $props();
</script>

{#if run}
	<div class="obs-overlay fixed inset-0 z-[70] flex items-center justify-center p-4">
		<button class="absolute inset-0" aria-label="Cancel deletion" onclick={onCancel}></button>
		<dialog open class="obs-dialog relative z-10 m-0 grid w-full max-w-md gap-4 rounded p-5">
			<h2 class="obs-heading text-lg font-semibold">Delete {run.metadata.level} run?</h2>
			<p class="obs-dim text-sm">Choose whether to preserve this run’s metadata for history and future statistics.</p>
			{#if error}<p class="text-sm text-(--obs-danger)">{error}</p>{/if}
			<div class="grid gap-2">
				{#if run.path}
					<button class="obs-button px-3 py-2 text-sm" disabled={busy} onclick={onDeleteVideo}
						>Delete video, keep run history</button
					>
				{/if}
				<button class="obs-button obs-button-danger px-3 py-2 text-sm" disabled={busy} onclick={onDeleteAll}>
					{run.path ? 'Delete video and run history' : 'Delete run history'}
				</button>
				<button class="obs-text-button px-3 py-2 text-sm" disabled={busy} onclick={onCancel}>Cancel</button>
			</div>
		</dialog>
	</div>
{/if}
