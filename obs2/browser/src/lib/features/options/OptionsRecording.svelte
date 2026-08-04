<script lang="ts">
	import Select from '$lib/ui/Select.svelte';
	import type { MonitorDesign } from '$lib/features/monitor/monitorView';
	import { settings } from '$lib/stores/settings.svelte';
	import {
		clipTemplateTokens,
		optionsClasses as styles,
		type RecordingOptionsView
	} from '$lib/features/options/optionsView';
	import Tooltip from '$lib/ui/Tooltip.svelte';

	const monitorDesignOptions: { value: MonitorDesign; label: string }[] = [
		{ value: 'signal-band', label: 'Signal band' },
		{ value: 'mission-glass', label: 'Mission glass' },
		{ value: 'debug', label: 'For Your Eyes Only' }
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
	<p class={styles.hint}>Change up the theme of the monitor shown while watching a capture source.</p>
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
		<p class="{styles.hint} leading-5">
			Use <span class={styles.templateToken}>{view.template.separator}</span> to create folders inside the output
			folder, for example
			<span class={styles.templateToken}
				>{`{level}${view.template.separator}{difficulty}${view.template.separator}{time}`}</span
			>.
		</p>
	{/if}
	<p class={styles.hint}>Available tokens</p>
	<div class="flex flex-wrap gap-2">
		{#each clipTemplateTokens as token}
			<Tooltip content={token.description} class="cursor-help">
				<code class={styles.templateToken} aria-label={`${token.value}: ${token.description}`}>{token.value}</code>
			</Tooltip>
		{/each}
	</div>
</section>

<section class={styles.panel}>
	<div class="flex flex-wrap items-center justify-between gap-3">
		<label class={styles.label} for="completed-output-path">Where to save clips?</label>
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
		<p class={styles.hint}>
			Defaults to a folder named <span class={styles.templateToken}>GoldenEye</span> inside OBS's replay-buffer output folder.
		</p>
	{/if}
</section>

<section class={styles.panel}>
	<label class={styles.label} for="recent-run-limit">Max recent clip limit</label>
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
		While monitoring your game, this plugin saves clips of your runs. This setting controls how many clips will be saved
		on disk while you're playing. You can choose to 'keep' clips (save them forever) but if you don't they'll be removed
		automatically when there are more than this setting.
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
