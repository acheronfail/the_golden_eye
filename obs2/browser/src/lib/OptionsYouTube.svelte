<script lang="ts">
	import { Select, settings, type YoutubeVisibility } from '$lib';
	import { optionsClasses as styles } from '$lib/optionsView';
	import { youtube } from '$lib/youtube.svelte';
	import YouTubeConnectButton from '$lib/YouTubeConnectButton.svelte';

	const visibilityOptions: { value: YoutubeVisibility; label: string }[] = [
		{ value: 'public', label: 'Public' },
		{ value: 'unlisted', label: 'Unlisted' },
		{ value: 'private', label: 'Private' }
	];
	let accountLabel = $derived(youtube.account?.name ?? youtube.account?.email ?? 'Connected');
	let accountEmail = $derived(
		youtube.account?.email && youtube.account?.email !== accountLabel ? youtube.account.email : null
	);

	const tokens = [
		'{obs_replay_name}',
		'{mission}',
		'{part}',
		'{levelNumber}',
		'{level}',
		'{time}',
		'{difficulty}',
		'{status}',
		'{timestamp}',
		'{timestamp_local}',
		'{datetime_local}',
		'{plugin_version}'
	];

	const disconnect = async () => {
		await youtube.disconnect();
	};
	const onVisibilityChange = (value: string) => {
		settings.youtubeVisibility = value as YoutubeVisibility;
		settings.saveImmediately();
	};
</script>

<section class={styles.panel}>
	<div class="grid gap-3">
		<h2 class={styles.label}>YouTube</h2>
		{#if !youtube.connected}
			<p class={styles.hint}>Connect YouTube to upload videos directly from the Runs screen.</p>
			<div class="flex justify-center py-3">
				<YouTubeConnectButton />
			</div>
			{#if !youtube.oauthConfigured}
				<p class="text-xs text-(--obs-danger)">YouTube OAuth is not configured in this build.</p>
			{/if}
		{:else}
			<div class="flex items-center justify-between gap-3">
				<div class="min-w-0 text-left">
					<p class="truncate font-mono text-xs text-(--obs-text)">Connected as {accountLabel}</p>
					{#if accountEmail}
						<p class="truncate text-xs text-(--obs-text-muted)">{accountEmail}</p>
					{/if}
				</div>
				<button
					type="button"
					class="obs-button obs-button-danger px-3 py-1.5 font-mono text-xs"
					disabled={youtube.disconnecting}
					onclick={disconnect}
				>
					{youtube.disconnecting ? 'Disconnecting...' : 'Disconnect YouTube'}
				</button>
			</div>

			<label class="grid gap-1">
				<span class={styles.label}>Visibility</span>
				<Select
					class="font-mono text-sm"
					value={settings.youtubeVisibility}
					options={visibilityOptions}
					onChange={onVisibilityChange}
				/>
			</label>

			<label class="grid gap-1">
				<span class={styles.label}>Title</span>
				<input
					class={styles.input}
					bind:value={settings.youtubeTitleTemplate}
					onblur={() => settings.saveImmediately()}
				/>
			</label>

			<label class="grid gap-1">
				<span class={styles.label}>Description</span>
				<textarea
					class={styles.textarea}
					bind:value={settings.youtubeDescriptionTemplate}
					onblur={() => settings.saveImmediately()}
				></textarea>
			</label>

			<div class="grid gap-2">
				<p class={styles.hint}>Supported tokens</p>
				<div class="flex flex-wrap gap-1.5">
					{#each tokens as token}
						<code class={styles.templateToken}>{token}</code>
					{/each}
				</div>
			</div>
		{/if}
		{#if youtube.error}
			<p class="text-xs text-(--obs-danger)">{youtube.error}</p>
		{/if}
	</div>
</section>
