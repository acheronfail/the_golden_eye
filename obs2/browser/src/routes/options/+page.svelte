<script lang="ts">
	import { DEFAULT_CLIP_FILENAME_TEMPLATE, DEFAULT_POST_RUN_PADDING_SECS, settings } from '$lib';

	const panelClass = 'rounded-md border border-neutral-800 bg-neutral-950/60 px-4 py-4';
	const labelClass = 'text-sm font-semibold text-amber-300';
	const hintClass = 'mt-1 font-mono text-xs text-neutral-500';
	const inputClass =
		'mt-2 w-full rounded-md border-neutral-700 bg-neutral-950 font-mono text-sm text-neutral-100 placeholder:text-neutral-700 focus:border-amber-400 focus:ring-amber-400';

	const normalizeFailedRunLimit = () => {
		const value = Number(settings.failedRunLimit);
		settings.failedRunLimit = Number.isFinite(value) ? Math.max(0, Math.trunc(value)) : 0;
	};

	const normalizePreRunPadding = () => {
		const value = Number(settings.preRunPaddingSecs);
		settings.preRunPaddingSecs = Number.isFinite(value) ? Math.max(0, value) : 0;
	};

	const normalizePostRunPadding = () => {
		const value = Number(settings.postRunPaddingSecs);
		settings.postRunPaddingSecs = Number.isFinite(value) ? Math.max(0, value) : DEFAULT_POST_RUN_PADDING_SECS;
	};
</script>

<svelte:head>
	<title>Options</title>
</svelte:head>

<main class="mx-auto w-full max-w-2xl px-6 py-12">
	<h1 class="text-2xl font-semibold text-amber-300">Options</h1>
	<p class="mt-2 mb-8 text-sm text-neutral-400">Recording settings are saved locally.</p>

	<div class="flex flex-col gap-4">
		<section class={panelClass}>
			<label class={labelClass} for="clip-filename-template">Clip filename template</label>
			<input
				id="clip-filename-template"
				type="text"
				bind:value={settings.clipFilenameTemplate}
				placeholder={DEFAULT_CLIP_FILENAME_TEMPLATE}
				class={inputClass}
			/>
			<p class={hintClass}>
				{`{replay} {level} {time_suffix} {failed_suffix} {status}`}
			</p>
		</section>

		<section class={panelClass}>
			<label class={labelClass} for="completed-output-path">Completed run clips</label>
			<input
				id="completed-output-path"
				type="text"
				bind:value={settings.completedOutputPath}
				placeholder="/home/bond/Videos/GoldenEye/completed"
				class={inputClass}
			/>
			<p class={hintClass}>Leave blank to save beside the OBS replay-buffer file.</p>
		</section>

		<section class={panelClass}>
			<label class="flex items-center gap-3">
				<input
					type="checkbox"
					bind:checked={settings.saveFailedRuns}
					class="rounded border-neutral-700 bg-neutral-950 text-amber-500 focus:ring-amber-400"
				/>
				<span class={labelClass}>Save failed runs</span>
			</label>

			{#if settings.saveFailedRuns}
				<div class="mt-5 grid gap-5">
					<div>
						<label class={labelClass} for="failed-output-path">Failed run clips</label>
						<input
							id="failed-output-path"
							type="text"
							bind:value={settings.failedOutputPath}
							placeholder="/home/bond/Videos/GoldenEye/failed"
							class={inputClass}
						/>
						<p class={hintClass}>Leave blank to use the completed-run clip folder.</p>
					</div>

					<div>
						<label class={labelClass} for="failed-run-limit">Failed run limit</label>
						<input
							id="failed-run-limit"
							type="number"
							min="0"
							step="1"
							bind:value={settings.failedRunLimit}
							onblur={normalizeFailedRunLimit}
							class={`${inputClass} max-w-40`}
						/>
						<p class={hintClass}>0 keeps all failed clips.</p>
					</div>
				</div>
			{/if}
		</section>

		<section class={panelClass}>
			<h2 class={labelClass}>Trim timing</h2>
			<div class="mt-4 grid gap-5 sm:grid-cols-2">
				<div>
					<label class={labelClass} for="pre-run-padding">Pre-run padding</label>
					<input
						id="pre-run-padding"
						type="number"
						min="0"
						step="0.25"
						bind:value={settings.preRunPaddingSecs}
						onblur={normalizePreRunPadding}
						class={inputClass}
					/>
					<p class={hintClass}>How much footage to keep before the run-start screen is detected.</p>
				</div>

				<div>
					<label class={labelClass} for="post-run-padding">Post-run padding</label>
					<input
						id="post-run-padding"
						type="number"
						min="0"
						step="0.25"
						bind:value={settings.postRunPaddingSecs}
						onblur={normalizePostRunPadding}
						class={inputClass}
					/>
					<p class={hintClass}>How long to keep recording after the stats screen appears.</p>
				</div>
			</div>
		</section>
	</div>
</main>
