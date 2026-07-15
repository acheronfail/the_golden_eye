<script lang="ts">
	import { Select, settings, type UpdateCheckInterval } from '$lib';

	type UpdateButtonPhase = 'check' | 'checking' | 'download' | 'downloading' | 'apply' | 'applying';

	let {
		panelClass,
		labelClass,
		hintClass,
		pathButtonClass,
		updateCheckIntervals,
		updatePhase,
		updateActionPending,
		onUpdateCheckIntervalChange,
		onCheckForUpdateNow,
		onDownloadUpdateNow,
		onApplyUpdateNow
	}: {
		panelClass: string;
		labelClass: string;
		hintClass: string;
		pathButtonClass: string;
		updateCheckIntervals: { value: UpdateCheckInterval; label: string }[];
		updatePhase: UpdateButtonPhase;
		updateActionPending: boolean;
		onUpdateCheckIntervalChange: (value: string) => void;
		onCheckForUpdateNow: () => void;
		onDownloadUpdateNow: () => void;
		onApplyUpdateNow: () => void;
	} = $props();
</script>

<section class={panelClass}>
	<label class={labelClass} for="update-check-interval">Check for plugin updates</label>
	<Select
		id="update-check-interval"
		class="font-mono text-sm"
		value={settings.updateCheckInterval}
		onChange={onUpdateCheckIntervalChange}
		options={updateCheckIntervals}
	/>
	<p class={hintClass}>
		Checks GitHub releases on app startup and shows a notice to download and install a newer version when one exists.
	</p>
</section>

<section class={panelClass}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.autoUpdateEnabled}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={labelClass}>Automatically install updates</span>
	</label>
	<p class={hintClass}>
		Applies a downloaded, checksum-verified update on its own once it's safe to do so (no monitoring or recording in
		progress). The plugin keeps running throughout -- no OBS restart needed.
	</p>
	<div>
		{#if updatePhase === 'apply' || updatePhase === 'applying'}
			<button type="button" class={pathButtonClass} disabled={updateActionPending} onclick={onApplyUpdateNow}
				>{updatePhase === 'applying' ? 'Applying…' : 'Apply update now'}</button
			>
		{:else if updatePhase === 'download' || updatePhase === 'downloading'}
			<button type="button" class={pathButtonClass} disabled={updateActionPending} onclick={onDownloadUpdateNow}
				>{updatePhase === 'downloading' ? 'Downloading…' : 'Download now'}</button
			>
		{:else}
			<button type="button" class={pathButtonClass} disabled={updateActionPending} onclick={onCheckForUpdateNow}
				>{updatePhase === 'checking' ? 'Checking…' : 'Check now'}</button
			>
		{/if}
	</div>
</section>

<section class={panelClass}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.stopReplayBufferWhenMonitorStopped}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={labelClass}>Stop replay buffer when monitor stopped</span>
	</label>
	<p class={hintClass}>Stops OBS's replay buffer after monitoring is turned off.</p>
</section>

<section class={panelClass}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.showMonitorFps}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={labelClass}>Show monitor FPS</span>
	</label>
	<p class={hintClass}>Shows monitor throughput while monitoring is active.</p>
</section>

<section class={panelClass}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.showDeveloperSettings}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={labelClass}>Show developer settings</span>
	</label>
	<p class={hintClass}>Shows the Developer link in the header.</p>
</section>
