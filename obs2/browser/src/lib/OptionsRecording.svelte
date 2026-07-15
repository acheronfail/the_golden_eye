<script lang="ts">
	import { settings } from '$lib';
	import type { FolderValidation } from '$lib/api';

	type PathKind = 'completed' | 'failed';

	const clipTemplateTokens = [
		{ value: '{obs_replay_name}', description: 'Original OBS replay-buffer filename without the extension.' },
		{ value: '{level}', description: 'GoldenEye level name, such as Dam, Facility, or Egypt.' },
		{ value: '{levelNumber}', description: 'GoldenEye level number from 1 through 20.' },
		{ value: '{time}', description: 'Run time as mm:ss when the stats screen was read.' },
		{ value: '{difficulty}', description: 'Difficulty name: Agent, Secret Agent, 00 Agent, or 007.' },
		{ value: '{status}', description: 'Run result: complete, failed, abort, or kia.' },
		{ value: '{timestamp}', description: 'ISO timestamp in UTC for when the run completed.' },
		{ value: '{timestamp_local}', description: 'ISO timestamp in local time for when the run completed.' }
	];

	let {
		panelClass,
		labelClass,
		hintClass,
		inputClass,
		pathButtonClass,
		pathStatusClass,
		pathPendingClass,
		pathErrorClass,
		templateTokenClass,
		pickingPath,
		completedPathValidating,
		failedPathValidating,
		completedValidation,
		failedValidation,
		clipTemplateSeparator,
		clipTemplateError,
		completedOutputPathPlaceholder,
		failedOutputPathPlaceholder,
		setClipFilenameTemplate,
		chooseOutputPath,
		clearOutputPath,
		clearPathValidation,
		validateOutputPath,
		folderStatusMessage,
		normalizeFailedRunLimit,
		normalizeMinimumFailedRunLength,
		normalizePreRunPadding,
		normalizePostRunPadding
	}: {
		panelClass: string;
		labelClass: string;
		hintClass: string;
		inputClass: string;
		pathButtonClass: string;
		pathStatusClass: string;
		pathPendingClass: string;
		pathErrorClass: string;
		templateTokenClass: string;
		pickingPath: PathKind | null;
		completedPathValidating: boolean;
		failedPathValidating: boolean;
		completedValidation: FolderValidation | null;
		failedValidation: FolderValidation | null;
		clipTemplateSeparator: string;
		clipTemplateError: string | null;
		completedOutputPathPlaceholder: string;
		failedOutputPathPlaceholder: string;
		setClipFilenameTemplate: (value: string) => void;
		chooseOutputPath: (kind: PathKind) => void;
		clearOutputPath: (kind: PathKind) => void;
		clearPathValidation: (kind: PathKind) => void;
		validateOutputPath: (kind: PathKind) => void;
		folderStatusMessage: (validation: FolderValidation) => string;
		normalizeFailedRunLimit: () => void;
		normalizeMinimumFailedRunLength: () => void;
		normalizePreRunPadding: () => void;
		normalizePostRunPadding: () => void;
	} = $props();
</script>

<section class={panelClass}>
	<label class={labelClass} for="clip-filename-template">Clip filename template</label>
	<input
		id="clip-filename-template"
		type="text"
		value={settings.clipFilenameTemplate}
		oninput={(event) => setClipFilenameTemplate((event.currentTarget as HTMLInputElement).value)}
		placeholder={settings.defaults.clipFilenameTemplate}
		aria-invalid={Boolean(clipTemplateError)}
		class={inputClass}
	/>
	{#if clipTemplateError}
		<p class={pathErrorClass}>{clipTemplateError}</p>
	{:else}
		<p class={hintClass}>
			Use {clipTemplateSeparator} to create folders inside the output folder, for example {`{level}${clipTemplateSeparator}{difficulty}${clipTemplateSeparator}{time}`}.
		</p>
	{/if}
	<p class={hintClass}>Available tokens</p>
	<div class="flex flex-wrap gap-2">
		{#each clipTemplateTokens as token}
			<code class={templateTokenClass} title={token.description} aria-label={`${token.value}: ${token.description}`}
				>{token.value}</code
			>
		{/each}
	</div>
</section>

<section class={panelClass}>
	<div class="flex flex-wrap items-center justify-between gap-3">
		<label class={labelClass} for="completed-output-path">Completed run clips</label>
		<div class="flex flex-wrap justify-end gap-2">
			<button
				type="button"
				class={pathButtonClass}
				disabled={pickingPath !== null}
				onclick={() => chooseOutputPath('completed')}
				>{pickingPath === 'completed' ? 'Choosing...' : 'Choose...'}</button
			>
			{#if settings.completedOutputPath.trim()}
				<button type="button" class={pathButtonClass} onclick={() => clearOutputPath('completed')}>Use default</button>
			{/if}
		</div>
	</div>
	<input
		id="completed-output-path"
		type="text"
		bind:value={settings.completedOutputPath}
		oninput={() => clearPathValidation('completed')}
		onblur={() => validateOutputPath('completed')}
		placeholder={completedOutputPathPlaceholder}
		class={inputClass}
	/>
	{#if completedPathValidating}
		<p class={pathPendingClass}>Checking folder...</p>
	{:else if completedValidation?.error}
		<p class={pathErrorClass}>{completedValidation.error}</p>
	{:else if completedValidation && settings.completedOutputPath.trim()}
		<p class={pathStatusClass}>{folderStatusMessage(completedValidation)}</p>
	{:else}
		<p class={hintClass}>Defaults to a GoldenEye folder inside OBS's replay-buffer output folder.</p>
	{/if}
</section>

<section class={panelClass}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.saveFailedRuns}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={labelClass}>Save failed runs</span>
	</label>

	{#if settings.saveFailedRuns}
		<div class="grid gap-5">
			<div class="grid gap-3">
				<div class="flex flex-wrap items-center justify-between gap-3">
					<label class={labelClass} for="failed-output-path">Failed run clips</label>
					<div class="flex flex-wrap justify-end gap-2">
						<button
							type="button"
							class={pathButtonClass}
							disabled={pickingPath !== null}
							onclick={() => chooseOutputPath('failed')}
							>{pickingPath === 'failed' ? 'Choosing...' : 'Choose...'}</button
						>
						{#if settings.failedOutputPath.trim()}
							<button type="button" class={pathButtonClass} onclick={() => clearOutputPath('failed')}
								>Use default</button
							>
						{/if}
					</div>
				</div>
				<input
					id="failed-output-path"
					type="text"
					bind:value={settings.failedOutputPath}
					oninput={() => clearPathValidation('failed')}
					onblur={() => validateOutputPath('failed')}
					placeholder={failedOutputPathPlaceholder}
					class={inputClass}
				/>
				{#if failedPathValidating}
					<p class={pathPendingClass}>Checking folder...</p>
				{:else if failedValidation?.error}
					<p class={pathErrorClass}>{failedValidation.error}</p>
				{:else if failedValidation && settings.failedOutputPath.trim()}
					<p class={pathStatusClass}>{folderStatusMessage(failedValidation)}</p>
				{:else}
					<p class={hintClass}>
						Defaults to a folder named after the completed-run clip folder with " - failed" appended, alongside it.
					</p>
				{/if}
			</div>

			<div class="grid gap-5 sm:grid-cols-2">
				<div class="grid gap-2">
					<label class={labelClass} for="failed-run-limit">How many failed runs to keep</label>
					<input
						id="failed-run-limit"
						type="number"
						min="0"
						step="1"
						bind:value={settings.failedRunLimit}
						onblur={normalizeFailedRunLimit}
						class={inputClass}
					/>
					<p class={hintClass}>
						Set to 0 to keep all failed clips. When the limit is reached the oldest clips are deleted first.
					</p>
				</div>

				<div class="grid gap-2">
					<label class={labelClass} for="minimum-failed-run-length">Minimum failed run length (seconds)</label>
					<input
						id="minimum-failed-run-length"
						type="number"
						min="0"
						step="0.25"
						bind:value={settings.minimumFailedRunLengthSecs}
						onblur={normalizeMinimumFailedRunLength}
						class={inputClass}
					/>
					<p class={hintClass}>
						Set to 0 to save every failed run. Uses the time displayed on the stats screen when available (or falls back
						to the time between seeing the start screen and then seeing the stats screen).
					</p>
				</div>
			</div>
		</div>
	{/if}
</section>

<section class={panelClass}>
	<h2 class={labelClass}>Trim timing</h2>
	<div class="grid gap-5 sm:grid-cols-2">
		<div class="grid gap-2">
			<label class={labelClass} for="pre-run-padding">Pre-run padding (seconds)</label>
			<input
				id="pre-run-padding"
				type="number"
				min="0"
				step="0.25"
				bind:value={settings.preRunPaddingSecs}
				onblur={normalizePreRunPadding}
				class={inputClass}
			/>
			<p class={hintClass}>How much footage to keep before the start screen is detected.</p>
		</div>

		<div class="grid gap-2">
			<label class={labelClass} for="post-run-padding">Post-run padding (seconds)</label>
			<input
				id="post-run-padding"
				type="number"
				min="0"
				step="0.25"
				bind:value={settings.postRunPaddingSecs}
				onblur={normalizePostRunPadding}
				class={inputClass}
			/>
			<p class={hintClass}>How much footage to keep after the stats screen appears.</p>
		</div>
	</div>
</section>
