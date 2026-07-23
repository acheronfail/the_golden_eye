<script lang="ts">
	import Select from '$lib/components/Select.svelte';
	import type { MonitorDesign } from '$lib/components/monitorView';
	import { settings } from '$lib/stores/settings.svelte';
	import { optionsClasses as styles, type RecordingOptionsView } from '$lib/utils/optionsView';

	const monitorDesignOptions: { value: MonitorDesign; label: string }[] = [
		{ value: 'signal-band', label: 'Signal band' },
		{ value: 'mission-glass', label: 'Mission glass' },
		{ value: 'debug', label: 'For Your Eyes Only' }
	];

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

	let { view }: { view: RecordingOptionsView } = $props();
</script>

<section class={styles.panel}>
	<label class={styles.label} for="monitor-design">Monitor design</label>
	<Select
		id="monitor-design"
		value={settings.monitorDesign}
		onChange={(value) => (settings.monitorDesign = value as MonitorDesign)}
		options={monitorDesignOptions}
		class="font-mono text-sm"
	/>
	<p class={styles.hint}>Choose the full-screen monitor shown while watching a capture source.</p>
</section>

<section class={styles.panel}>
	<label class={styles.label} for="clip-filename-template">Clip filename template</label>
	<input
		id="clip-filename-template"
		type="text"
		value={settings.clipFilenameTemplate}
		oninput={(event) => view.template.set((event.currentTarget as HTMLInputElement).value)}
		placeholder={settings.defaults.clipFilenameTemplate}
		aria-invalid={Boolean(view.template.error)}
		class={styles.input}
	/>
	{#if view.template.error}
		<p class={styles.pathError}>{view.template.error}</p>
	{:else}
		<p class={styles.hint}>
			Use {view.template.separator} to create folders inside the output folder, for example {`{level}${view.template.separator}{difficulty}${view.template.separator}{time}`}.
		</p>
	{/if}
	<p class={styles.hint}>Available tokens</p>
	<div class="flex flex-wrap gap-2">
		{#each clipTemplateTokens as token}
			<code class={styles.templateToken} title={token.description} aria-label={`${token.value}: ${token.description}`}
				>{token.value}</code
			>
		{/each}
	</div>
</section>

<section class={styles.panel}>
	<div class="flex flex-wrap items-center justify-between gap-3">
		<label class={styles.label} for="completed-output-path">Run clips</label>
		<div class="flex flex-wrap justify-end gap-2">
			<button type="button" class={styles.pathButton} disabled={view.paths.picking} onclick={view.paths.choose}
				>{view.paths.picking ? 'Choosing...' : 'Choose...'}</button
			>
			{#if settings.completedOutputPath.trim()}
				<button type="button" class={styles.pathButton} onclick={view.paths.clear}>Use default</button>
			{/if}
		</div>
	</div>
	<input
		id="completed-output-path"
		type="text"
		bind:value={settings.completedOutputPath}
		oninput={view.paths.clearValidation}
		onblur={view.paths.validate}
		placeholder={view.paths.placeholder}
		class={styles.input}
	/>
	{#if view.paths.validating}
		<p class={styles.pathPending}>Checking folder...</p>
	{:else if view.paths.validation?.error}
		<p class={styles.pathError}>{view.paths.validation.error}</p>
	{:else if view.paths.validation && settings.completedOutputPath.trim()}
		<p class={styles.pathStatus}>{view.paths.statusMessage(view.paths.validation)}</p>
	{:else}
		<p class={styles.hint}>Defaults to a GoldenEye folder inside OBS's replay-buffer output folder.</p>
	{/if}
</section>

<section class={styles.panel}>
	<label class={styles.label} for="recent-run-limit">Recent run history</label>
	<input
		id="recent-run-limit"
		type="number"
		min="1"
		max="20"
		step="1"
		bind:value={settings.recentRunLimit}
		onblur={view.normalize.recentRunLimit}
		class={styles.input}
	/>
	<p class={styles.hint}>
		Keep videos for this many recent runs while you decide what to keep. After each new clip is saved, older unkept
		videos are removed, but their run history remains.
	</p>
</section>

<section class={styles.panel}>
	<h2 class={styles.label}>Trim timing</h2>
	<div class="grid gap-5 sm:grid-cols-2">
		<div class="grid gap-2">
			<label class={styles.label} for="pre-run-padding">Pre-run padding (seconds)</label>
			<input
				id="pre-run-padding"
				type="number"
				min="0"
				step="0.25"
				bind:value={settings.preRunPaddingSecs}
				onblur={view.normalize.preRunPadding}
				class={styles.input}
			/>
			<p class={styles.hint}>How much footage to keep before the start screen is detected.</p>
		</div>

		<div class="grid gap-2">
			<label class={styles.label} for="post-run-padding">Post-run padding (seconds)</label>
			<input
				id="post-run-padding"
				type="number"
				min="0"
				step="0.25"
				bind:value={settings.postRunPaddingSecs}
				onblur={view.normalize.postRunPadding}
				class={styles.input}
			/>
			<p class={styles.hint}>How much footage to keep after the stats screen appears.</p>
		</div>
	</div>
</section>
