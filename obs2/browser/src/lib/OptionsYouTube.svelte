<script lang="ts">
	import { Select, settings, type YoutubeVisibility } from '$lib';
	import { youtube } from '$lib/youtube.svelte';
	import YouTubeConnectButton from '$lib/YouTubeConnectButton.svelte';

	let {
		panelClass,
		labelClass,
		hintClass,
		inputClass,
		textareaClass,
		templateTokenClass
	}: {
		panelClass: string;
		labelClass: string;
		hintClass: string;
		inputClass: string;
		textareaClass: string;
		templateTokenClass: string;
	} = $props();

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

<section class={panelClass}>
	<div class="grid gap-3">
		<h2 class={labelClass}>YouTube</h2>
		{#if !youtube.connected}
			<p class={hintClass}>Connect YouTube to upload videos directly from the Runs screen.</p>
			<div class="flex justify-center py-3">
				<YouTubeConnectButton />
			</div>
			{#if !youtube.oauthConfigured}
				<p class="text-xs text-(--obs-danger)">YouTube OAuth is not configured in this build.</p>
			{/if}
		{:else}
			<div class="flex items-center justify-between gap-3">
				<div class="min-w-0 text-left">
					<p class="truncate font-mono text-xs text-[var(--obs-text)]">Connected as {accountLabel}</p>
					{#if accountEmail}
						<p class="truncate text-xs text-[var(--obs-text-muted)]">{accountEmail}</p>
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
				<span class={labelClass}>Visibility</span>
				<Select
					class="font-mono text-sm"
					value={settings.youtubeVisibility}
					options={visibilityOptions}
					onChange={onVisibilityChange}
				/>
			</label>

			<label class="grid gap-1">
				<span class={labelClass}>Title</span>
				<input
					class={inputClass}
					bind:value={settings.youtubeTitleTemplate}
					onblur={() => settings.saveImmediately()}
				/>
			</label>

			<label class="grid gap-1">
				<span class={labelClass}>Description</span>
				<textarea
					class={textareaClass}
					bind:value={settings.youtubeDescriptionTemplate}
					onblur={() => settings.saveImmediately()}
				></textarea>
			</label>

			<div class="grid gap-2">
				<p class={hintClass}>Supported tokens</p>
				<div class="flex flex-wrap gap-1.5">
					{#each tokens as token}
						<code class={templateTokenClass}>{token}</code>
					{/each}
				</div>
			</div>
		{/if}
		{#if youtube.error}
			<p class="text-xs text-(--obs-danger)">{youtube.error}</p>
		{/if}
	</div>
</section>
