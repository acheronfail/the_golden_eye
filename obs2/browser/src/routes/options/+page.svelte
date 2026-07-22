<script lang="ts">
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { backend, type FolderValidation } from '$lib/api';
	import { dismissNotificationFlagsByKey } from '$lib/stores/notifications.svelte';
	import { replayBuffer } from '$lib/stores/replayBuffer.svelte';
	import Select from '$lib/components/Select.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import OptionsGeneral from '$lib/components/OptionsGeneral.svelte';
	import OptionsNotifications from '$lib/components/OptionsNotifications.svelte';
	import OptionsRecording from '$lib/components/OptionsRecording.svelte';
	import OptionsYouTube from '$lib/components/OptionsYouTube.svelte';
	import ResetSettingsDialog from '$lib/components/ResetSettingsDialog.svelte';
	import { optionsClasses, type OptionsPathKind, type RecordingOptionsView } from '$lib/utils/optionsView';
	import { youtube } from '$lib/stores/youtube.svelte';

	type OptionsTab = 'general' | 'recording' | 'notifications' | 'youtube';
	const OPTIONS_TAB_STORAGE_KEY = 'the-golden-eye.options-tab';

	const optionSections = $derived<{ value: OptionsTab; label: string }[]>([
		{ value: 'general', label: 'General' },
		{ value: 'recording', label: 'Recording' },
		{ value: 'notifications', label: 'Notifications' },
		...(youtube.enabled ? [{ value: 'youtube' as const, label: 'YouTube' }] : [])
	]);

	const tabFromUrl = (value: string | null): OptionsTab =>
		value === 'recording' || value === 'notifications' || (value === 'youtube' && youtube.enabled) ? value : 'general';

	let activeTab = $state<OptionsTab>(tabFromUrl(page.url.searchParams.get('tab')));
	let pickingPath: OptionsPathKind | null = $state(null);
	let revealingConfigFile = $state(false);
	let resettingConfigFile = $state(false);
	let showResetConfirmation = $state(false);
	let configActionError = $state<string | null>(null);
	let completedPathValidating = $state(false);
	let failedPathValidating = $state(false);
	let completedValidation: FolderValidation | null = $state(null);
	let failedValidation: FolderValidation | null = $state(null);
	let completedValidationSeq = 0;
	let failedValidationSeq = 0;
	let clipTemplateSeparator = $state('/');

	const rememberTab = (tab: OptionsTab) => {
		if (browser) localStorage.setItem(OPTIONS_TAB_STORAGE_KEY, tab);
	};

	onMount(() => {
		const requestedTab = page.url.searchParams.get('tab');
		const tab = tabFromUrl(requestedTab ?? localStorage.getItem(OPTIONS_TAB_STORAGE_KEY));
		activeTab = tab;
		rememberTab(tab);
	});

	$effect(() => {
		const requestedTab = page.url.searchParams.get('tab');
		if (requestedTab === null) return;
		const tab = tabFromUrl(requestedTab);
		activeTab = tab;
		rememberTab(tab);
	});

	const selectTab = (tab: OptionsTab) => {
		activeTab = tab;
		rememberTab(tab);
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

	const { panel: panelClass, label: labelClass, hint: hintClass, pathButton: pathButtonClass } = optionsClasses;
	const dangerPanelClass =
		'grid gap-3 rounded border border-(--obs-danger) bg-[color-mix(in_srgb,var(--obs-danger)_14%,transparent)] px-4 py-4';
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

	const outputPath = (kind: OptionsPathKind): string =>
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

	const setOutputPath = (kind: OptionsPathKind, value: string) => {
		if (kind === 'completed') {
			settings.completedOutputPath = value;
		} else {
			settings.failedOutputPath = value;
		}
	};

	const setPathValidation = (kind: OptionsPathKind, validation: FolderValidation | null) => {
		if (kind === 'completed') {
			completedValidation = validation;
		} else {
			failedValidation = validation;
		}
	};

	const setPathValidating = (kind: OptionsPathKind, value: boolean) => {
		if (kind === 'completed') {
			completedPathValidating = value;
		} else {
			failedPathValidating = value;
		}
	};

	const nextValidationSeq = (kind: OptionsPathKind): number =>
		kind === 'completed' ? ++completedValidationSeq : ++failedValidationSeq;

	const currentValidationSeq = (kind: OptionsPathKind): number =>
		kind === 'completed' ? completedValidationSeq : failedValidationSeq;

	const clearPathValidation = (kind: OptionsPathKind) => {
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

	const validateOutputPath = async (kind: OptionsPathKind) => {
		const value = outputPath(kind).trim();
		const seq = nextValidationSeq(kind);

		if (!value) {
			setPathValidation(kind, null);
			setPathValidating(kind, false);
			return;
		}

		setPathValidating(kind, true);
		try {
			const validation = await backend.validateFolder(value);
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

	const chooseOutputPath = async (kind: OptionsPathKind) => {
		const currentPath =
			kind === 'failed'
				? settings.failedOutputPath.trim() || failedOutputPathPlaceholder
				: settings.completedOutputPath.trim() || completedOutputPathPlaceholder;

		pickingPath = kind;
		try {
			const result = await backend.pickFolder({
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

	const clearOutputPath = (kind: OptionsPathKind) => {
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

	const requestConfigReset = () => {
		configActionError = null;
		showResetConfirmation = true;
	};

	const cancelConfigReset = () => {
		if (!resettingConfigFile) showResetConfirmation = false;
	};

	const resetConfigFile = async () => {
		resettingConfigFile = true;
		configActionError = null;
		try {
			await settings.resetToDefaults();
			dismissNotificationFlagsByKey('settings-config-error');
			showResetConfirmation = false;
		} catch (err) {
			configActionError = errorMessage(err);
		} finally {
			resettingConfigFile = false;
		}
	};

	let recordingOptionsView = $derived<RecordingOptionsView>({
		template: {
			separator: clipTemplateSeparator,
			error: clipTemplateError,
			set: setClipFilenameTemplate
		},
		paths: {
			picking: pickingPath,
			completed: {
				validating: completedPathValidating,
				validation: completedValidation,
				placeholder: completedOutputPathPlaceholder
			},
			failed: {
				validating: failedPathValidating,
				validation: failedValidation,
				placeholder: failedOutputPathPlaceholder
			},
			choose: chooseOutputPath,
			clear: clearOutputPath,
			clearValidation: clearPathValidation,
			validate: validateOutputPath,
			statusMessage: folderStatusMessage
		},
		normalize: {
			preRunPadding: normalizePreRunPadding,
			postRunPadding: normalizePostRunPadding
		}
	});
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
				<button type="button" class={pathButtonClass} disabled={resettingConfigFile} onclick={requestConfigReset}>
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
			<OptionsGeneral />
		{:else if activeTab === 'recording'}
			<OptionsRecording view={recordingOptionsView} />
		{:else if activeTab === 'notifications'}
			<OptionsNotifications />
		{:else}
			<OptionsYouTube />
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
				<div class="flex flex-wrap justify-end gap-2">
					<button type="button" class={pathButtonClass} disabled={revealingConfigFile} onclick={showConfigFile}>
						{revealingConfigFile ? 'Opening...' : 'show config file'}
					</button>
					<button type="button" class="obs-button obs-button-danger px-3 py-1.5 text-xs" onclick={requestConfigReset}>
						Reset to defaults
					</button>
				</div>
			</div>
		</section>
	{/if}
</main>

{#if showResetConfirmation}
	<ResetSettingsDialog
		busy={resettingConfigFile}
		error={configActionError}
		cancel={cancelConfigReset}
		reset={resetConfigFile}
	/>
{/if}
