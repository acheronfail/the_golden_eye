<script lang="ts">
	import { Select, settings } from '$lib';
	import { optionsClasses as styles, type GeneralOptionsView } from '$lib/optionsView';

	let { view }: { view: GeneralOptionsView } = $props();
</script>

<section class={styles.panel}>
	<label class={styles.label} for="update-check-interval">Check for plugin updates</label>
	<Select
		id="update-check-interval"
		class="font-mono text-sm"
		value={settings.updateCheckInterval}
		onChange={view.update.setInterval}
		options={view.update.intervals}
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
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={styles.label}>Automatically install updates</span>
	</label>
	<p class={styles.hint}>
		Applies a downloaded, checksum-verified update on its own once it's safe to do so (no monitoring or recording in
		progress). The plugin keeps running throughout -- no OBS restart needed.
	</p>
	<div>
		{#if view.update.phase === 'apply' || view.update.phase === 'applying'}
			<button type="button" class={styles.pathButton} disabled={view.update.pending} onclick={view.update.apply}
				>{view.update.phase === 'applying' ? 'Applying…' : 'Apply update now'}</button
			>
		{:else if view.update.phase === 'download' || view.update.phase === 'downloading'}
			<button type="button" class={styles.pathButton} disabled={view.update.pending} onclick={view.update.download}
				>{view.update.phase === 'downloading' ? 'Downloading…' : 'Download now'}</button
			>
		{:else}
			<button type="button" class={styles.pathButton} disabled={view.update.pending} onclick={view.update.check}
				>{view.update.phase === 'checking' ? 'Checking…' : 'Check now'}</button
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
