<script lang="ts">
	import Select from '$lib/components/Select.svelte';
	import { settings, type UpdateCheckInterval } from '$lib/stores/settings.svelte';
	import { updates } from '$lib/stores/updates.svelte';
	import { monitor } from '$lib/stores/monitor.svelte';
	import { optionsClasses as styles } from '$lib/utils/optionsView';

	const updateCheckIntervals: { value: UpdateCheckInterval; label: string }[] = [
		{ value: 'monthly', label: 'Monthly' },
		{ value: 'weekly', label: 'Weekly' },
		{ value: 'daily', label: 'Daily' },
		{ value: 'never', label: 'Never' }
	];

	const onUpdateCheckIntervalChange = (value: string) => {
		settings.updateCheckInterval = value as UpdateCheckInterval;
	};

	let applyBlockedReason = $derived(
		monitor.status?.enabled ? "The update can't be applied while the monitor is active." : null
	);
	let manualUpdate = $derived(
		updates.status.phase === 'available' && updates.status.available?.requiresManualInstall
			? updates.status.available
			: null
	);
</script>

<section class={styles.panel}>
	<label class={styles.label} for="update-check-interval">Check for plugin updates</label>
	<Select
		id="update-check-interval"
		class="font-mono text-sm"
		value={settings.updateCheckInterval}
		onChange={onUpdateCheckIntervalChange}
		options={updateCheckIntervals}
	/>
	<p class={styles.hint}>
		Checks GitHub releases on app startup and shows a notice to download and install a newer version when one exists.
	</p>
</section>

<section class={styles.panel}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.autoUpdateEnabled}
			disabled={manualUpdate !== null}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={styles.label}>Automatically install updates</span>
	</label>
	{#if manualUpdate}
		<p class={styles.hint}>
			Version {manualUpdate.latestVersion} requires a manual install, so automatic updates are temporarily unavailable. Your
			preference will be kept.
		</p>
	{:else}
		<p class={styles.hint}>
			Applies a downloaded, checksum-verified update on its own once it's safe to do so (no monitoring or recording in
			progress). The plugin keeps running throughout -- no OBS restart needed.
		</p>
	{/if}
	<div>
		{#if manualUpdate}
			<button type="button" class={styles.pathButton} onclick={() => updates.openAvailableRelease()}
				>Open release page</button
			>
		{:else if updates.buttonPhase === 'apply' || updates.buttonPhase === 'applying'}
			<button
				type="button"
				class={styles.pathButton}
				disabled={updates.pending || applyBlockedReason !== null}
				onclick={() => updates.apply()}>{updates.buttonPhase === 'applying' ? 'Applying…' : 'Apply update now'}</button
			>
			{#if applyBlockedReason}
				<p class={`${styles.hint} mt-2`}>{applyBlockedReason}</p>
			{/if}
		{:else if updates.buttonPhase === 'download' || updates.buttonPhase === 'downloading'}
			<button type="button" class={styles.pathButton} disabled={updates.pending} onclick={() => updates.download()}
				>{updates.buttonPhase === 'downloading' ? 'Downloading…' : 'Download now'}</button
			>
		{:else}
			<button type="button" class={styles.pathButton} disabled={updates.pending} onclick={() => updates.check()}
				>{updates.buttonPhase === 'checking' ? 'Checking…' : 'Check now'}</button
			>
		{/if}
	</div>
</section>

<section class={styles.panel}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.stopReplayBufferWhenMonitorStopped}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={styles.label}>Stop replay buffer when monitor stopped</span>
	</label>
	<p class={styles.hint}>Stops OBS's replay buffer after monitoring is turned off.</p>
</section>

<section class={styles.panel}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.showMonitorFps}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={styles.label}>Show monitor FPS</span>
	</label>
	<p class={styles.hint}>Shows monitor throughput while monitoring is active.</p>
</section>

<section class={styles.panel}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.showDeveloperSettings}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={styles.label}>Show developer settings</span>
	</label>
	<p class={styles.hint}>Shows the Developer link in the header.</p>
</section>
