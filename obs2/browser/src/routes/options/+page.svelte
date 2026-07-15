<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		applyUpdateNow,
		checkForUpdateNow,
		downloadUpdateNow,
		getUpdateStatus,
		pickFolder,
		validateFolder,
		type FolderValidation
	} from '$lib/api';
	import { addNotificationFlag, dismissNotificationFlagsByKey } from '$lib/notifications.svelte';
	import { replayBuffer } from '$lib/replayBuffer.svelte';
	import { Select, settings, type UpdateCheckInterval } from '$lib';

	type OptionsTab = 'general' | 'recording' | 'notifications';
	type PathKind = 'completed' | 'failed';

	const optionSections: { value: OptionsTab; label: string }[] = [
		{ value: 'general', label: 'General' },
		{ value: 'recording', label: 'Recording' },
		{ value: 'notifications', label: 'Notifications' }
	];

	const tabFromUrl = (value: string | null): OptionsTab =>
		value === 'recording' || value === 'notifications' ? value : 'general';

	let activeTab = $derived(tabFromUrl(page.url.searchParams.get('tab')));
	let pickingPath: PathKind | null = $state(null);
	let revealingConfigFile = $state(false);
	let resettingConfigFile = $state(false);
	let configActionError = $state<string | null>(null);
	let completedPathValidating = $state(false);
	let failedPathValidating = $state(false);
	let completedValidation: FolderValidation | null = $state(null);
	let failedValidation: FolderValidation | null = $state(null);
	let completedValidationSeq = 0;
	let failedValidationSeq = 0;
	let clipTemplateSeparator = $state('/');

	const selectTab = (tab: OptionsTab) => {
		const url = new URL(page.url);
		if (tab === 'general') {
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

	const onSectionChange = (value: string) => {
		selectTab(value as OptionsTab);
	};

	const panelClass = 'obs-panel grid gap-3 rounded px-4 py-4';
	const dangerPanelClass =
		'grid gap-3 rounded border border-[var(--obs-danger)] bg-[color-mix(in_srgb,var(--obs-danger)_14%,transparent)] px-4 py-4';
	const labelClass = 'text-sm font-semibold';
	const hintClass = 'obs-dim font-mono text-xs';
	const inputClass = 'obs-input font-mono text-sm disabled:cursor-not-allowed disabled:opacity-50';
	const textareaClass = `${inputClass} min-h-24 resize-y`;
	const pathButtonClass =
		'obs-button px-3 py-1.5 text-xs whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-50';
	const pathStatusClass = 'text-xs text-[var(--obs-success)]';
	const pathPendingClass = 'obs-dim break-all font-mono text-xs';
	const pathErrorClass = 'break-words text-xs text-[var(--obs-danger)]';
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
	const updateCheckIntervals: { value: UpdateCheckInterval; label: string }[] = [
		{ value: 'monthly', label: 'Monthly' },
		{ value: 'weekly', label: 'Weekly' },
		{ value: 'daily', label: 'Daily' },
		{ value: 'never', label: 'Never' }
	];

	const onUpdateCheckIntervalChange = (value: string) => {
		settings.updateCheckInterval = value as UpdateCheckInterval;
	};

	// Drives the update button's label and click action: check (idle), checking,
	// download (update found, auto-install off), downloading, apply (verified
	// update staged), applying.
	type UpdateButtonPhase = 'check' | 'checking' | 'download' | 'downloading' | 'apply' | 'applying';
	let updatePhase = $state<UpdateButtonPhase>('check');
	const updateActionPending = $derived(
		updatePhase === 'checking' || updatePhase === 'downloading' || updatePhase === 'applying'
	);

	// Polls the staging status until a verified update is ready or the timeout
	// elapses. Tolerates fetch errors (e.g. the server briefly dropping mid
	// swap) by treating them as "not yet." Returns whether one became ready.
	const pollUntilStaged = async (timeoutMs: number): Promise<boolean> => {
		const deadline = Date.now() + timeoutMs;
		while (Date.now() < deadline) {
			try {
				if ((await getUpdateStatus()).staged) return true;
			} catch (err) {
				console.warn('Failed to fetch update status', err);
			}
			await new Promise((resolve) => setTimeout(resolve, 1000));
		}
		return false;
	};

	// Runs once on mount (nothing reactive is read, so $effect never re-fires):
	// reflect an update already staged in the background as "apply now," since
	// there's no push-based "something got staged" signal to react to.
	$effect(() => {
		void (async () => {
			try {
				if ((await getUpdateStatus()).staged) updatePhase = 'apply';
			} catch (err) {
				console.warn('Failed to fetch update status', err);
			}
		})();
	});

	const onCheckForUpdateNow = async () => {
		updatePhase = 'checking';
		try {
			const { update } = await checkForUpdateNow();
			if (!update) {
				addNotificationFlag({ title: "You're up to date", tone: 'success' });
				updatePhase = 'check';
				return;
			}
			if (!settings.autoUpdateEnabled) {
				// No automatic download -- let the user start it explicitly. No toast:
				// the backend pushes the sticky "update available" notice and the
				// button flips to "Download now," so a second toast is just noise.
				updatePhase = 'download';
				return;
			}
			// Auto-install is on: the backend is already downloading, so wait for it
			// to stage. The sticky notice is suppressed here, so this toast is the
			// only feedback for the interaction.
			addNotificationFlag({
				title: 'Update found',
				detail: `Downloading and verifying ${update.latestVersion}...`,
				tone: 'info'
			});
			updatePhase = 'downloading';
			if (await pollUntilStaged(30_000)) {
				updatePhase = 'apply';
			} else {
				addNotificationFlag({
					title: 'Still downloading',
					detail: "It's taking longer than expected -- finish it from the button.",
					tone: 'info'
				});
				// Offer an actionable button rather than a stuck spinner; the
				// download endpoint just finishes what's already in flight.
				updatePhase = 'download';
			}
		} catch (err) {
			addNotificationFlag({
				title: 'Update check failed',
				detail: err instanceof Error ? err.message : String(err),
				tone: 'error'
			});
			updatePhase = 'check';
		}
	};

	const onDownloadUpdateNow = async () => {
		updatePhase = 'downloading';
		try {
			// Resolves only once the update is downloaded, verified, and staged.
			await downloadUpdateNow();
			updatePhase = 'apply';
		} catch (err) {
			addNotificationFlag({
				title: 'Update download failed',
				detail: err instanceof Error ? err.message : String(err),
				tone: 'error'
			});
			updatePhase = 'download';
		}
	};

	const onApplyUpdateNow = async () => {
		updatePhase = 'applying';
		try {
			await applyUpdateNow();
			addNotificationFlag({
				title: 'Applying update',
				detail: 'The plugin will briefly reconnect while the update is installed.',
				tone: 'success'
			});
			// The swap happens in the background and briefly drops the HTTP server,
			// so keep the button disabled and poll (tolerating connection errors)
			// until the staged update is gone.
			const deadline = Date.now() + 20_000;
			let stillStaged = true;
			while (Date.now() < deadline && stillStaged) {
				await new Promise((resolve) => setTimeout(resolve, 1000));
				try {
					stillStaged = (await getUpdateStatus()).staged;
				} catch {
					// Server is briefly gone mid-swap; keep waiting.
				}
			}
			updatePhase = stillStaged ? 'apply' : 'check';
		} catch (err) {
			addNotificationFlag({
				title: 'Could not apply update',
				detail: err instanceof Error ? err.message : String(err),
				tone: 'error'
			});
			// The failure might mean it was never staged in the first place
			// (e.g. it got applied or cleared by another client) -- reflect the
			// real status rather than leaving a stale "Apply update now" showing.
			try {
				updatePhase = (await getUpdateStatus()).staged ? 'apply' : 'check';
			} catch {
				updatePhase = 'apply';
			}
		}
	};

	const normalizeFailedRunLimit = () => {
		const value = Number(settings.failedRunLimit);
		settings.failedRunLimit = Number.isFinite(value) ? Math.max(0, Math.trunc(value)) : 0;
	};

	const normalizeMinimumFailedRunLength = () => {
		const value = Number(settings.minimumFailedRunLengthSecs);
		settings.minimumFailedRunLengthSecs = Number.isFinite(value)
			? Math.max(0, value)
			: settings.defaults.minimumFailedRunLengthSecs;
	};

	const normalizePreRunPadding = () => {
		const value = Number(settings.preRunPaddingSecs);
		settings.preRunPaddingSecs = Number.isFinite(value) ? Math.max(0, value) : settings.defaults.preRunPaddingSecs;
	};

	const normalizePostRunPadding = () => {
		const value = Number(settings.postRunPaddingSecs);
		settings.postRunPaddingSecs = Number.isFinite(value) ? Math.max(0, value) : settings.defaults.postRunPaddingSecs;
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

	const siblingFailedPath = (base: string): string => {
		const trimmed = base.trim().replace(/[\\/]+$/, '');
		return trimmed ? `${trimmed} - failed` : '';
	};

	const completedDefaultOutputPath = (): string =>
		replayBuffer.status?.defaultCompletedOutputPath ??
		(replayBuffer.status?.outputDirectory ? joinPath(replayBuffer.status.outputDirectory, 'GoldenEye') : '');

	let completedOutputPathPlaceholder = $derived(completedDefaultOutputPath() || 'OBS replay folder/GoldenEye');
	let failedOutputPathPlaceholder = $derived(
		siblingFailedPath(settings.completedOutputPath.trim() || completedOutputPathPlaceholder) ||
			replayBuffer.status?.defaultFailedOutputPath ||
			'GoldenEye - failed'
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

	const showConfigFile = async () => {
		revealingConfigFile = true;
		configActionError = null;
		try {
			await settings.revealConfigFile();
		} catch (err) {
			configActionError = errorMessage(err);
		} finally {
			revealingConfigFile = false;
		}
	};

	const resetConfigFile = async () => {
		resettingConfigFile = true;
		configActionError = null;
		try {
			await settings.resetToDefaults();
			dismissNotificationFlagsByKey('settings-config-error');
		} catch (err) {
			configActionError = errorMessage(err);
		} finally {
			resettingConfigFile = false;
		}
	};
</script>

<svelte:head>
	<title>Options</title>
</svelte:head>

<main class="mx-auto grid w-full max-w-2xl gap-5 px-4 py-8 sm:px-6 sm:py-12">
	<div class="grid gap-2">
		<h1 class="obs-heading text-2xl font-semibold">Options</h1>
		<p class="obs-subtitle text-sm">Settings are saved automatically.</p>
	</div>

	{#if settings.fileError}
		<section class={dangerPanelClass}>
			<div class="flex flex-wrap items-start justify-between gap-3">
				<div class="grid min-w-0 flex-1 gap-3">
					<h2 class="text-sm font-semibold text-(--obs-danger)">Config file error</h2>
					{#if settings.configPath}
						<p class="obs-dim font-mono text-xs break-all">{settings.configPath}</p>
					{/if}
					<pre
						class="text(--obs-danger) max-h-52 overflow-auto font-mono text-xs wrap-break-word whitespace-pre-wrap">{settings.fileError}</pre>
				</div>
				<button type="button" class={pathButtonClass} disabled={resettingConfigFile} onclick={resetConfigFile}>
					{resettingConfigFile ? 'Resetting...' : 'Reset to defaults'}
				</button>
			</div>
			<p class={hintClass}>Options are disabled until the JSON file is fixed or reset.</p>
		</section>
	{/if}

	{#if configActionError}
		<p class="text-xs text-(--obs-danger)">{configActionError}</p>
	{/if}

	<div class="flex items-center gap-3">
		<label for="options-section" class="obs-dim shrink-0 font-mono text-xs tracking-wide uppercase">Section</label>
		<Select
			id="options-section"
			class="min-w-0 flex-1 font-mono text-sm sm:max-w-60"
			value={activeTab}
			onChange={onSectionChange}
			options={optionSections}
		/>
	</div>

	<fieldset disabled={!settings.canEdit} class="flex flex-col gap-4 border-0 p-0">
		{#if activeTab === 'general'}
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
					Checks GitHub releases on app startup and shows a notice to download and install a newer version when one
					exists.
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
					Applies a downloaded, checksum-verified update on its own once it's safe to do so (no monitoring or recording
					in progress). The plugin keeps running throughout -- no OBS restart needed.
				</p>
				<div>
					{#if updatePhase === 'apply' || updatePhase === 'applying'}
						<button type="button" class={pathButtonClass} disabled={updateActionPending} onclick={onApplyUpdateNow}>
							{updatePhase === 'applying' ? 'Applying…' : 'Apply update now'}
						</button>
					{:else if updatePhase === 'download' || updatePhase === 'downloading'}
						<button type="button" class={pathButtonClass} disabled={updateActionPending} onclick={onDownloadUpdateNow}>
							{updatePhase === 'downloading' ? 'Downloading…' : 'Download now'}
						</button>
					{:else}
						<button type="button" class={pathButtonClass} disabled={updateActionPending} onclick={onCheckForUpdateNow}>
							{updatePhase === 'checking' ? 'Checking…' : 'Check now'}
						</button>
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
		{:else if activeTab === 'recording'}
			<section class={panelClass}>
				<label class={labelClass} for="clip-filename-template">Clip filename template</label>
				<input
					id="clip-filename-template"
					type="text"
					value={settings.clipFilenameTemplate}
					oninput={(event) => setClipFilenameTemplate((event.currentTarget as HTMLInputElement).value)}
					placeholder={settings.defaults.clipFilenameTemplate}
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
				<div class="flex flex-wrap gap-2">
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
					<p class={hintClass}>Defaults to a GoldenEye folder inside OBS's replay-buffer output folder.</p>
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
					<div class="grid gap-5">
						<div class="grid gap-3">
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
								<p class={hintClass}>Defaults to a folder named after the completed-run clip folder with " - failed" appended, alongside it.</p>
							{/if}
						</div>

						<div class="grid gap-5 sm:grid-cols-2">
							<div class="grid gap-2">
								<label class={labelClass} for="failed-run-limit">How many failed runs to keep</label>
								<input
									id="failed-run-limit"
									type="number"
									min="0"
									step="1"
									bind:value={settings.failedRunLimit}
									onblur={normalizeFailedRunLimit}
									class={inputClass}
								/>
								<p class={hintClass}>
									Set to 0 to keep all failed clips. When the limit is reached the oldest clips are deleted first.
								</p>
							</div>

							<div class="grid gap-2">
								<label class={labelClass} for="minimum-failed-run-length">Minimum failed run length (seconds)</label>
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
									Set to 0 to save every failed run. Uses the time displayed on the stats screen when available (or
									falls back to the time between seeing the start screen and then seeing the stats screen).
								</p>
							</div>
						</div>
					</div>
				{/if}
			</section>

			<section class={panelClass}>
				<h2 class={labelClass}>Trim timing</h2>
				<div class="grid gap-5 sm:grid-cols-2">
					<div class="grid gap-2">
						<label class={labelClass} for="pre-run-padding">Pre-run padding (seconds)</label>
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

					<div class="grid gap-2">
						<label class={labelClass} for="post-run-padding">Post-run padding (seconds)</label>
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
						placeholder={settings.defaults.streamingStartedMessageTemplate}
						class={textareaClass}
					></textarea>
					<p class={hintClass}>Available tokens</p>
					<div class="flex flex-wrap gap-2">
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
						placeholder={settings.defaults.streamingStoppedMessageTemplate}
						class={textareaClass}
					></textarea>
					<p class={hintClass}>Available tokens</p>
					<div class="flex flex-wrap gap-2">
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

	{#if activeTab === 'general'}
		<section class={panelClass}>
			<div class="flex flex-wrap items-center justify-between gap-3">
				<div class="grid min-w-0 gap-1">
					<h2 class={labelClass}>Configuration file</h2>
					{#if settings.configPath}
						<p class="obs-dim font-mono text-xs break-all">{settings.configPath}</p>
					{:else}
						<p class={hintClass}>Open the settings JSON file in the system file explorer.</p>
					{/if}
				</div>
				<button type="button" class={pathButtonClass} disabled={revealingConfigFile} onclick={showConfigFile}>
					{revealingConfigFile ? 'Opening...' : 'show config file'}
				</button>
			</div>
		</section>
	{/if}
</main>
