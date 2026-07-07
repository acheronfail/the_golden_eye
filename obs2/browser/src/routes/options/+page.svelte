<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { pickFolder, validateFolder, type FolderValidation } from '$lib/api';
	import { replayBuffer } from '$lib/replayBuffer.svelte';
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

	const optionSections: { value: OptionsTab; label: string }[] = [
		{ value: 'general', label: 'General' },
		{ value: 'recording', label: 'Recording' },
		{ value: 'notifications', label: 'Notifications' }
	];

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
	let clipTemplateSeparator = $state('/');

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

	const onSectionChange = (event: Event) => {
		selectTab((event.currentTarget as HTMLSelectElement).value as OptionsTab);
	};

	const panelClass = 'obs-panel rounded px-4 py-4';
	const labelClass = 'text-sm font-semibold';
	const hintClass = 'obs-dim mt-1 font-mono text-xs';
	const inputClass = 'obs-input mt-2 font-mono text-sm disabled:cursor-not-allowed disabled:opacity-50';
	const textareaClass = `${inputClass} min-h-24 resize-y`;
	const pathButtonClass =
		'obs-button px-3 py-1.5 text-xs whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-50';
	const pathStatusClass = 'mt-2 text-xs text-[var(--obs-success)]';
	const pathPendingClass = 'obs-dim mt-2 break-all font-mono text-xs';
	const pathErrorClass = 'mt-2 break-words text-xs text-[var(--obs-danger)]';
	const templateTokenClass = 'obs-token cursor-help break-all rounded px-1.5 py-1 font-mono text-xs';
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

	const normalizeMinimumFailedRunLength = () => {
		const value = Number(settings.minimumFailedRunLengthSecs);
		settings.minimumFailedRunLengthSecs = Number.isFinite(value) ? Math.max(0, value) : 0;
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

	$effect(() => {
		if (typeof navigator !== 'undefined') {
			clipTemplateSeparator = navigator.platform.toLowerCase().includes('win') ? '\\' : '/';
		}
	});

	const wrongClipTemplateSeparator = $derived(clipTemplateSeparator === '/' ? '\\' : '/');
	const clipTemplateError = $derived(
		validateClipFilenameTemplate(settings.clipFilenameTemplate, clipTemplateSeparator, wrongClipTemplateSeparator)
	);

	const setClipFilenameTemplate = (value: string) => {
		settings.clipFilenameTemplate = value.split(wrongClipTemplateSeparator).join(clipTemplateSeparator);
	};

	function validateClipFilenameTemplate(value: string, separator: string, wrongSeparator: string): string | null {
		const trimmed = value.trim();
		if (!trimmed) return null;
		if (trimmed.includes(wrongSeparator)) {
			return `Use ${separator} as the folder separator on this platform.`;
		}
		if (trimmed.startsWith(separator)) {
			return 'Template paths must be relative to the configured output folder.';
		}

		const parts = trimmed.split(separator);
		if (parts.some((part) => part.trim() === '')) {
			return 'Folder names in the template cannot be empty.';
		}
		if (parts.some((part) => part.trim() === '.' || part.trim() === '..')) {
			return 'Template paths can only go into child folders.';
		}
		return null;
	}

	const outputPath = (kind: PathKind): string =>
		kind === 'completed' ? settings.completedOutputPath : settings.failedOutputPath;

	const joinPath = (base: string, child: string): string => {
		const trimmed = base.trim();
		if (!trimmed) return '';

		const separator = trimmed.includes('\\') && !trimmed.includes('/') ? '\\' : '/';
		return `${trimmed.replace(/[\\/]+$/, '')}${separator}${child}`;
	};

	const completedDefaultOutputPath = (): string =>
		replayBuffer.status?.defaultCompletedOutputPath ??
		(replayBuffer.status?.outputDirectory ? joinPath(replayBuffer.status.outputDirectory, 'Goldeneye') : '');

	let completedOutputPathPlaceholder = $derived(completedDefaultOutputPath() || 'OBS replay folder/Goldeneye');
	let failedOutputPathPlaceholder = $derived(
		joinPath(settings.completedOutputPath.trim() || completedOutputPathPlaceholder, 'failed') ||
			replayBuffer.status?.defaultFailedOutputPath ||
			'completed clip folder/failed'
	);

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
				? settings.failedOutputPath.trim() || failedOutputPathPlaceholder
				: settings.completedOutputPath.trim() || completedOutputPathPlaceholder;

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

<main class="mx-auto w-full max-w-2xl px-4 py-8 sm:px-6 sm:py-12">
	<h1 class="obs-heading text-2xl font-semibold">Options</h1>
	<p class="obs-subtitle mt-2 mb-8 text-sm">Settings are saved automatically.</p>

	<div class="mb-5 flex items-center gap-3">
		<label for="options-section" class="obs-dim shrink-0 font-mono text-xs tracking-wide uppercase">Section</label>
		<select
			id="options-section"
			class="obs-select min-w-0 flex-1 font-mono text-sm sm:max-w-60"
			value={activeTab}
			onchange={onSectionChange}
		>
			{#each optionSections as section}
				<option value={section.value}>{section.label}</option>
			{/each}
		</select>
	</div>

	<fieldset disabled={!settings.loaded} class="m-0 flex flex-col gap-4 border-0 p-0">
		{#if activeTab === 'general'}
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
		{:else if activeTab === 'recording'}
			<section class={panelClass}>
				<label class={labelClass} for="clip-filename-template">Clip filename template</label>
				<input
					id="clip-filename-template"
					type="text"
					value={settings.clipFilenameTemplate}
					oninput={(event) => setClipFilenameTemplate((event.currentTarget as HTMLInputElement).value)}
					placeholder={DEFAULT_CLIP_FILENAME_TEMPLATE}
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
					<div class="flex flex-wrap justify-end gap-2">
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
								Use default
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
					<p class={hintClass}>Defaults to a Goldeneye folder inside OBS's replay-buffer output folder.</p>
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
					<div class="mt-5 grid gap-5">
						<div>
							<div class="flex flex-wrap items-center justify-between gap-3">
								<label class={labelClass} for="failed-output-path">Failed run clips</label>
								<div class="flex flex-wrap justify-end gap-2">
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
											Use default
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
								<p class={hintClass}>Defaults to a failed folder inside the completed-run clip folder.</p>
							{/if}
						</div>

						<div class="grid gap-5 sm:grid-cols-2">
							<div>
								<label class={labelClass} for="failed-run-limit">Failed run limit</label>
								<input
									id="failed-run-limit"
									type="number"
									min="0"
									step="1"
									bind:value={settings.failedRunLimit}
									onblur={normalizeFailedRunLimit}
									class={inputClass}
								/>
								<p class={hintClass}>0 keeps all failed clips.</p>
							</div>

							<div>
								<label class={labelClass} for="minimum-failed-run-length">Minimum failed run length</label>
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
									0 saves all failed runs. Uses the time displayed on the stats screen when available (or falls back to
									the time between seeing the start screen and then seeing the stats screen).
								</p>
							</div>
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
						<p class={hintClass}>How much footage to keep before the start screen is detected.</p>
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
						<p class={hintClass}>How much footage to keep after the stats screen appears.</p>
					</div>
				</div>
			</section>
		{:else}
			<section class={panelClass}>
				<label class="flex items-center gap-3">
					<input
						type="checkbox"
						bind:checked={settings.discordNotificationsEnabled}
						class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
					/>
					<span class={labelClass}>Enable Discord notifications</span>
				</label>
				<p class={hintClass}>Enable notifications in Discord for streaming, requires a webhook URL.</p>
			</section>

			{#if settings.discordNotificationsEnabled}
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
		{/if}
	</fieldset>
</main>
