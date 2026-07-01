<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		DEFAULT_CLIP_FILENAME_TEMPLATE,
		DEFAULT_POST_RUN_PADDING_SECS,
		DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE,
		DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE,
		settings
	} from '$lib';

	type OptionsTab = 'recording' | 'notifications';

	const tabFromUrl = (value: string | null): OptionsTab => (value === 'notifications' ? 'notifications' : 'recording');

	let activeTab = $derived(tabFromUrl(page.url.searchParams.get('tab')));

	const selectTab = (tab: OptionsTab) => {
		const url = new URL(page.url);
		if (tab === 'recording') {
			url.searchParams.delete('tab');
		} else {
			url.searchParams.set('tab', tab);
		}
		void goto(`${url.pathname}${url.search}${url.hash}`, {
			replaceState: true,
			noScroll: true,
			keepFocus: true
		});
	};

	const panelClass = 'rounded-md border border-neutral-800 bg-neutral-950/60 px-4 py-4';
	const labelClass = 'text-sm font-semibold text-amber-300';
	const hintClass = 'mt-1 font-mono text-xs text-neutral-500';
	const inputClass =
		'mt-2 w-full rounded-md border-neutral-700 bg-neutral-950 font-mono text-sm text-neutral-100 placeholder:text-neutral-700 focus:border-amber-400 focus:ring-amber-400 disabled:cursor-not-allowed disabled:opacity-50';
	const textareaClass = `${inputClass} min-h-24 resize-y`;
	const templateTokenClass =
		'cursor-help rounded border border-neutral-800 bg-neutral-900 px-1.5 py-1 font-mono text-xs text-neutral-300';
	const tabBaseClass =
		'border-b-2 px-4 py-2 font-mono text-sm transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-amber-400';
	const tabClass = (tab: OptionsTab) =>
		activeTab === tab
			? `${tabBaseClass} border-amber-400 text-amber-300`
			: `${tabBaseClass} border-transparent text-neutral-400 hover:text-amber-300`;
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
	const notificationTemplateTokens = [
		{ value: '{broadcast_url}', description: 'YouTube broadcast URL for the stream.' },
		{ value: '{timestamp}', description: 'ISO timestamp in UTC for when the stream event was handled.' },
		{ value: '{timestamp_local}', description: 'ISO timestamp in local time for when the stream event was handled.' },
		{ value: '{unix_seconds}', description: 'Unix timestamp in seconds, suitable for Discord timestamp markup.' }
	];

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
	<p class="mt-2 mb-8 text-sm text-neutral-400">Settings are saved by the plugin.</p>

	<div class="mb-6 flex border-b border-neutral-800" role="tablist" aria-label="Options sections">
		<button
			type="button"
			role="tab"
			aria-selected={activeTab === 'recording'}
			class={tabClass('recording')}
			onclick={() => selectTab('recording')}
		>
			Recording
		</button>
		<button
			type="button"
			role="tab"
			aria-selected={activeTab === 'notifications'}
			class={tabClass('notifications')}
			onclick={() => selectTab('notifications')}
		>
			Notifications
		</button>
	</div>

	<fieldset disabled={!settings.loaded} class="m-0 flex flex-col gap-4 border-0 p-0">
		{#if activeTab === 'recording'}
			<section class={panelClass}>
				<label class={labelClass} for="clip-filename-template">Clip filename template</label>
				<input
					id="clip-filename-template"
					type="text"
					bind:value={settings.clipFilenameTemplate}
					placeholder={DEFAULT_CLIP_FILENAME_TEMPLATE}
					class={inputClass}
				/>
				<p class={hintClass}>Available tokens</p>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each clipTemplateTokens as token}
						<code
							class={templateTokenClass}
							title={token.description}
							aria-label={`${token.value}: ${token.description}`}
						>
							{token.value}
						</code>
					{/each}
				</div>
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
						class="rounded border-neutral-700 bg-neutral-950 text-amber-500 focus:ring-amber-400 disabled:cursor-not-allowed disabled:opacity-50"
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
		{:else}
			<section class={panelClass}>
				<label class="flex items-center gap-3">
					<input
						type="checkbox"
						bind:checked={settings.discordNotificationsEnabled}
						class="rounded border-neutral-700 bg-neutral-950 text-amber-500 focus:ring-amber-400 disabled:cursor-not-allowed disabled:opacity-50"
					/>
					<span class={labelClass}>Enable Discord notifications</span>
				</label>
				<p class={hintClass}>Disable notifications without clearing the saved webhook URL.</p>
			</section>

			<section class={panelClass}>
				<label class={labelClass} for="discord-webhook-url">Discord webhook URL</label>
				<input
					id="discord-webhook-url"
					type="url"
					bind:value={settings.discordWebhookUrl}
					placeholder="https://discord.com/api/webhooks/..."
					autocomplete="off"
					spellcheck="false"
					class={inputClass}
				/>
			</section>

			<section class={panelClass}>
				<label class={labelClass} for="streaming-started-message-template">Streaming started message template</label>
				<textarea
					id="streaming-started-message-template"
					rows="3"
					bind:value={settings.streamingStartedMessageTemplate}
					placeholder={DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE}
					class={textareaClass}
				></textarea>
				<p class={hintClass}>Available tokens</p>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each notificationTemplateTokens as token}
						<code
							class={templateTokenClass}
							title={token.description}
							aria-label={`${token.value}: ${token.description}`}
						>
							{token.value}
						</code>
					{/each}
				</div>
			</section>

			<section class={panelClass}>
				<label class={labelClass} for="streaming-stopped-message-template">Streaming stopped message template</label>
				<textarea
					id="streaming-stopped-message-template"
					rows="3"
					bind:value={settings.streamingStoppedMessageTemplate}
					placeholder={DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE}
					class={textareaClass}
				></textarea>
				<p class={hintClass}>Available tokens</p>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each notificationTemplateTokens as token}
						<code
							class={templateTokenClass}
							title={token.description}
							aria-label={`${token.value}: ${token.description}`}
						>
							{token.value}
						</code>
					{/each}
				</div>
			</section>
		{/if}
	</fieldset>
</main>
