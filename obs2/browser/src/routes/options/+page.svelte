<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { pickFolder, validateFolder, type FolderValidation } from '$lib/api';
	import {
		DEFAULT_CLIP_FILENAME_TEMPLATE,
		DEFAULT_POST_RUN_PADDING_SECS,
		DEFAULT_PRE_RUN_PADDING_SECS,
		DEFAULT_STREAMING_STARTED_MESSAGE_TEMPLATE,
		DEFAULT_STREAMING_STOPPED_MESSAGE_TEMPLATE,
		settings
	} from '$lib';

	type OptionsTab = 'general' | 'recording' | 'notifications';
	type PathKind = 'completed' | 'failed';

	const tabFromUrl = (value: string | null): OptionsTab =>
		value === 'general' || value === 'notifications' ? value : 'recording';

	let activeTab = $derived(tabFromUrl(page.url.searchParams.get('tab')));
	let pickingPath: PathKind | null = $state(null);
	let completedPathValidating = $state(false);
	let failedPathValidating = $state(false);
	let completedValidation: FolderValidation | null = $state(null);
	let failedValidation: FolderValidation | null = $state(null);
	let completedValidationSeq = 0;
	let failedValidationSeq = 0;

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
	const pathButtonClass =
		'rounded-md border border-neutral-700 bg-neutral-900 px-3 py-1.5 text-xs font-semibold whitespace-nowrap text-neutral-200 transition-colors hover:border-amber-500 hover:text-amber-300 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-amber-400 disabled:cursor-not-allowed disabled:opacity-50';
	const pathStatusClass = 'mt-2 text-xs text-emerald-400';
	const pathPendingClass = 'mt-2 break-all font-mono text-xs text-neutral-500';
	const pathErrorClass = 'mt-2 break-words text-xs text-red-300';
	const templateTokenClass =
		'cursor-help rounded border border-neutral-800 bg-neutral-900 px-1.5 py-1 font-mono text-xs text-neutral-300';
	const tabBaseClass =
		'border-b-2 px-4 py-2 font-mono text-sm transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-amber-400';
	const tabClass = (tab: OptionsTab) =>
		activeTab === tab
			? `${tabBaseClass} border-amber-400 text-amber-300`
			: `${tabBaseClass} border-transparent text-neutral-400 hover:text-amber-300`;
	const clipTemplateTokens = [
		{
			value: '{obs_replay_name}',
			description: 'Original OBS replay-buffer filename without the extension.'
		},
		{ value: '{level}', description: 'GoldenEye level name, such as Dam, Facility, or Egypt.' },
		{ value: '{levelNumber}', description: 'GoldenEye level number from 1 through 20.' },
		{ value: '{time}', description: 'Run time as mm:ss when the stats screen was read.' },
		{
			value: '{difficulty}',
			description: 'Difficulty name: Agent, Secret Agent, 00 Agent, or 007.'
		},
		{ value: '{status}', description: 'Run result: complete, failed, abort, or kia.' },
		{ value: '{timestamp}', description: 'ISO timestamp in UTC for when the run completed.' },
		{
			value: '{timestamp_local}',
			description: 'ISO timestamp in local time for when the run completed.'
		}
	];
	const notificationTemplateTokens = [
		{ value: '{broadcast_url}', description: 'YouTube broadcast URL for the stream.' },
		{
			value: '{timestamp}',
			description: 'ISO timestamp in UTC for when the stream event was handled.'
		},
		{
			value: '{timestamp_local}',
			description: 'ISO timestamp in local time for when the stream event was handled.'
		},
		{
			value: '{unix_seconds}',
			description: 'Unix timestamp in seconds, suitable for Discord timestamp markup.'
		}
	];

	const normalizeFailedRunLimit = () => {
		const value = Number(settings.failedRunLimit);
		settings.failedRunLimit = Number.isFinite(value) ? Math.max(0, Math.trunc(value)) : 0;
	};

	const normalizePreRunPadding = () => {
		const value = Number(settings.preRunPaddingSecs);
		settings.preRunPaddingSecs = Number.isFinite(value) ? Math.max(0, value) : DEFAULT_PRE_RUN_PADDING_SECS;
	};

	const normalizePostRunPadding = () => {
		const value = Number(settings.postRunPaddingSecs);
		settings.postRunPaddingSecs = Number.isFinite(value) ? Math.max(0, value) : DEFAULT_POST_RUN_PADDING_SECS;
	};

	const errorMessage = (err: unknown): string => (err instanceof Error ? err.message : String(err));

	const outputPath = (kind: PathKind): string =>
		kind === 'completed' ? settings.completedOutputPath : settings.failedOutputPath;

	const setOutputPath = (kind: PathKind, value: string) => {
		if (kind === 'completed') {
			settings.completedOutputPath = value;
		} else {
			settings.failedOutputPath = value;
		}
	};

	const setPathValidation = (kind: PathKind, validation: FolderValidation | null) => {
		if (kind === 'completed') {
			completedValidation = validation;
		} else {
			failedValidation = validation;
		}
	};

	const setPathValidating = (kind: PathKind, value: boolean) => {
		if (kind === 'completed') {
			completedPathValidating = value;
		} else {
			failedPathValidating = value;
		}
	};

	const nextValidationSeq = (kind: PathKind): number =>
		kind === 'completed' ? ++completedValidationSeq : ++failedValidationSeq;

	const currentValidationSeq = (kind: PathKind): number =>
		kind === 'completed' ? completedValidationSeq : failedValidationSeq;

	const clearPathValidation = (kind: PathKind) => {
		nextValidationSeq(kind);
		setPathValidation(kind, null);
		setPathValidating(kind, false);
	};

	const pathValidationError = (message: string): FolderValidation => ({
		expandedPath: '',
		empty: false,
		exists: false,
		isDirectory: false,
		writable: false,
		willCreate: false,
		error: message
	});

	const validateOutputPath = async (kind: PathKind) => {
		const value = outputPath(kind).trim();
		const seq = nextValidationSeq(kind);

		if (!value) {
			setPathValidation(kind, null);
			setPathValidating(kind, false);
			return;
		}

		setPathValidating(kind, true);
		try {
			const validation = await validateFolder(value);
			if (seq === currentValidationSeq(kind) && value === outputPath(kind).trim()) {
				setPathValidation(kind, validation);
			}
		} catch (err) {
			if (seq === currentValidationSeq(kind)) {
				setPathValidation(kind, pathValidationError(errorMessage(err)));
			}
		} finally {
			if (seq === currentValidationSeq(kind)) {
				setPathValidating(kind, false);
			}
		}
	};

	const chooseOutputPath = async (kind: PathKind) => {
		const currentPath =
			kind === 'failed'
				? settings.failedOutputPath.trim() || settings.completedOutputPath.trim()
				: settings.completedOutputPath.trim();

		pickingPath = kind;
		try {
			const result = await pickFolder({
				title: kind === 'completed' ? 'Choose completed clips folder' : 'Choose failed clips folder',
				currentPath
			});
			if (!result.cancelled && result.path) {
				setOutputPath(kind, result.path);
				await validateOutputPath(kind);
			}
		} catch (err) {
			setPathValidation(kind, pathValidationError(errorMessage(err)));
		} finally {
			pickingPath = null;
		}
	};

	const folderStatusMessage = (validation: FolderValidation): string =>
		validation.willCreate ? 'Ready: folder will be created' : 'Ready: folder exists';

	const clearOutputPath = (kind: PathKind) => {
		setOutputPath(kind, '');
		clearPathValidation(kind);
	};
</script>

<svelte:head>
	<title>Options</title>
</svelte:head>

<main class="mx-auto w-full max-w-2xl px-6 py-12">
	<h1 class="text-2xl font-semibold text-amber-300">Options</h1>
	<p class="mt-2 mb-8 text-sm text-neutral-400">Settings are saved automatically.</p>

	<div class="mb-6 flex border-b border-neutral-800" role="tablist" aria-label="Options sections">
		<button
			type="button"
			role="tab"
			aria-selected={activeTab === 'general'}
			class={tabClass('general')}
			onclick={() => selectTab('general')}
		>
			General
		</button>
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
		{#if activeTab === 'general'}
			<section class={panelClass}>
				<label class="flex items-center gap-3">
					<input
						type="checkbox"
						bind:checked={settings.openGoldenEyeOnLaunch}
						class="rounded border-neutral-700 bg-neutral-950 text-amber-500 focus:ring-amber-400 disabled:cursor-not-allowed disabled:opacity-50"
					/>
					<span class={labelClass}>Open The Golden Eye when OBS launches</span>
				</label>
				<p class={hintClass}>Opens the plugin dashboard in your default browser.</p>
			</section>

			<section class={panelClass}>
				<label class="flex items-center gap-3">
					<input
						type="checkbox"
						bind:checked={settings.stopReplayBufferWhenMonitorStopped}
						class="rounded border-neutral-700 bg-neutral-950 text-amber-500 focus:ring-amber-400 disabled:cursor-not-allowed disabled:opacity-50"
					/>
					<span class={labelClass}>Stop replay buffer when monitor stopped</span>
				</label>
				<p class={hintClass}>Stops OBS's replay buffer after monitoring is turned off.</p>
			</section>
		{:else if activeTab === 'recording'}
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
				<div class="flex flex-wrap items-center justify-between gap-3">
					<label class={labelClass} for="completed-output-path">Completed run clips</label>
					<div class="flex gap-2">
						<button
							type="button"
							class={pathButtonClass}
							disabled={pickingPath !== null}
							onclick={() => chooseOutputPath('completed')}
						>
							{pickingPath === 'completed' ? 'Choosing...' : 'Choose...'}
						</button>
						{#if settings.completedOutputPath.trim()}
							<button type="button" class={pathButtonClass} onclick={() => clearOutputPath('completed')}>
								Clear
							</button>
						{/if}
					</div>
				</div>
				<input
					id="completed-output-path"
					type="text"
					bind:value={settings.completedOutputPath}
					oninput={() => clearPathValidation('completed')}
					onblur={() => validateOutputPath('completed')}
					placeholder="/home/bond/Videos/GoldenEye/completed"
					class={inputClass}
				/>
				{#if completedPathValidating}
					<p class={pathPendingClass}>Checking folder...</p>
				{:else if completedValidation?.error}
					<p class={pathErrorClass}>{completedValidation.error}</p>
				{:else if completedValidation && settings.completedOutputPath.trim()}
					<p class={pathStatusClass}>{folderStatusMessage(completedValidation)}</p>
				{:else}
					<p class={hintClass}>Leave blank to save beside the OBS replay-buffer file.</p>
				{/if}
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
							<div class="flex flex-wrap items-center justify-between gap-3">
								<label class={labelClass} for="failed-output-path">Failed run clips</label>
								<div class="flex gap-2">
									<button
										type="button"
										class={pathButtonClass}
										disabled={pickingPath !== null}
										onclick={() => chooseOutputPath('failed')}
									>
										{pickingPath === 'failed' ? 'Choosing...' : 'Choose...'}
									</button>
									{#if settings.failedOutputPath.trim()}
										<button type="button" class={pathButtonClass} onclick={() => clearOutputPath('failed')}>
											Use completed
										</button>
									{/if}
								</div>
							</div>
							<input
								id="failed-output-path"
								type="text"
								bind:value={settings.failedOutputPath}
								oninput={() => clearPathValidation('failed')}
								onblur={() => validateOutputPath('failed')}
								placeholder="/home/bond/Videos/GoldenEye/failed"
								class={inputClass}
							/>
							{#if failedPathValidating}
								<p class={pathPendingClass}>Checking folder...</p>
							{:else if failedValidation?.error}
								<p class={pathErrorClass}>{failedValidation.error}</p>
							{:else if failedValidation && settings.failedOutputPath.trim()}
								<p class={pathStatusClass}>{folderStatusMessage(failedValidation)}</p>
							{:else}
								<p class={hintClass}>Leave blank to use the completed-run clip folder.</p>
							{/if}
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
				<p class={hintClass}>Enable notifications in Discord for streaming, requires a webhook URL.</p>
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
