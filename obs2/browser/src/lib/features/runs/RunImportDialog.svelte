<script lang="ts">
	import type { ManualRunInput, RomVersion, TheEliteImportResponse } from '$lib/api';
	import SegmentedControl from '$lib/ui/SegmentedControl.svelte';
	import Select from '$lib/ui/Select.svelte';
	import {
		DIFFICULTY_OPTIONS,
		gameLanguageForRomVersion,
		LANGUAGE_OPTIONS,
		LEVEL_OPTIONS,
		OPTIONAL_ROM_VERSION_OPTIONS
	} from '$lib/features/runs/runsView';

	let {
		open,
		busy = null,
		error = null,
		result = null,
		initialMode = 'manual',
		onClose,
		onManual,
		onElite
	}: {
		open: boolean;
		busy?: 'manual' | 'elite' | null;
		error?: string | null;
		result?: TheEliteImportResponse | null;
		initialMode?: 'manual' | 'elite';
		onClose: () => void;
		onManual: (input: ManualRunInput) => void | Promise<void>;
		onElite: (username: string) => void | Promise<void>;
	} = $props();

	let mode = $state<'manual' | 'elite'>('manual');
	let date = $state(new Date().toISOString().slice(0, 10));
	let level = $state('');
	let difficulty = $state('');
	let time = $state('');
	let gameLanguage = $state('en');
	let romVersion = $state<RomVersion | ''>('');
	let youtubeUrl = $state('');
	let username = $state('');
	let manualValid = $derived(Boolean(date && level && difficulty && time && gameLanguage));

	$effect(() => {
		if (open) mode = initialMode;
	});

	const submitManual = (event: SubmitEvent) => {
		event.preventDefault();
		void onManual({
			date,
			level,
			difficulty,
			time,
			gameLanguage,
			romVersion: romVersion || undefined,
			youtubeUrl: youtubeUrl.trim() || undefined
		});
	};
	const submitElite = (event: SubmitEvent) => {
		event.preventDefault();
		void onElite(username.trim());
	};
	const changeRomVersion = (value: string) => {
		romVersion = value as RomVersion | '';
		gameLanguage = gameLanguageForRomVersion(romVersion) ?? gameLanguage;
	};
</script>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center obs-overlay p-4">
		<button type="button" aria-label="Close add times dialog" class="absolute inset-0 cursor-default" onclick={onClose}
		></button>
		<dialog
			open
			aria-label="Add times"
			class="relative z-10 m-0 max-h-full w-full max-w-xl overflow-hidden rounded obs-dialog p-0"
		>
			<header class="flex items-start gap-3 obs-dialog-header px-4 py-3">
				<div class="min-w-0 flex-1">
					<h2 class="text-lg font-semibold obs-heading">Add times</h2>
					<p class="mt-1 text-xs obs-dim">Add one previous run or import your complete GoldenEye PR history.</p>
				</div>
				<button
					type="button"
					class="obs-text-button px-1.5 py-0.5 text-xs"
					aria-label="Close add times dialog"
					onclick={onClose}
				>
					x
				</button>
			</header>

			<div class="max-h-[calc(100vh-9rem)] overflow-y-auto p-4">
				<SegmentedControl
					bind:value={mode}
					options={[
						{ value: 'manual', label: 'Add manually' },
						{ value: 'elite', label: 'Import from The Elite' }
					]}
					ariaLabel="Time import method"
				/>

				{#if mode === 'manual'}
					<form class="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2" onsubmit={submitManual}>
						<label class="flex flex-col gap-1">
							<span class="font-mono text-xs obs-dim">Date achieved</span>
							<input class="obs-input px-3 py-2 font-mono" type="date" required bind:value={date} />
						</label>
						<label class="flex flex-col gap-1">
							<span class="font-mono text-xs obs-dim">Time</span>
							<input
								class="obs-input px-3 py-2 font-mono"
								required
								bind:value={time}
								inputmode="numeric"
								pattern="[0-9]+:[0-5][0-9]"
								placeholder="mm:ss"
							/>
						</label>
						<label class="flex flex-col gap-1">
							<span class="font-mono text-xs obs-dim">Level</span>
							<Select
								class="w-full"
								placeholder="select level"
								bind:value={level}
								options={LEVEL_OPTIONS.map((value) => ({ value, label: value }))}
							/>
						</label>
						<label class="flex flex-col gap-1">
							<span class="font-mono text-xs obs-dim">Difficulty</span>
							<Select
								class="w-full"
								placeholder="select difficulty"
								bind:value={difficulty}
								options={DIFFICULTY_OPTIONS}
							/>
						</label>
						<div class="grid grid-cols-1 gap-3 sm:col-span-2 sm:grid-cols-2">
							<label class="flex flex-col gap-1">
								<span class="font-mono text-xs obs-dim">ROM version <span class="normal-case">(optional)</span></span>
								<Select
									class="w-full"
									placeholder="not set"
									bind:value={romVersion}
									options={OPTIONAL_ROM_VERSION_OPTIONS}
									onChange={changeRomVersion}
								/>
							</label>
							<label class="flex flex-col gap-1">
								<span class="font-mono text-xs obs-dim">Game language</span>
								<Select
									class="w-full"
									bind:value={gameLanguage}
									options={LANGUAGE_OPTIONS}
									disabled={Boolean(romVersion)}
								/>
							</label>
						</div>
						<label class="flex flex-col gap-1 sm:col-span-2">
							<span class="font-mono text-xs obs-dim">YouTube link <span class="normal-case">(optional)</span></span>
							<input
								class="obs-input px-3 py-2 font-mono"
								type="url"
								bind:value={youtubeUrl}
								placeholder="https://www.youtube.com/watch?v=..."
							/>
							<span class="text-[11px] obs-dim">Use this when the previous run is already uploaded.</span>
						</label>
						<div class="flex justify-end sm:col-span-2">
							<button
								type="submit"
								disabled={busy !== null || !manualValid}
								class="obs-button obs-button-gold px-3 py-2 font-mono text-xs disabled:opacity-50"
							>
								{busy === 'manual' ? 'adding...' : 'add time'}
							</button>
						</div>
					</form>
				{:else}
					<form class="mt-4 grid gap-4" onsubmit={submitElite}>
						<div class="rounded obs-empty-state px-4 py-3 text-sm">
							<p class="font-semibold">Import all GoldenEye personal-record history</p>
							<p class="mt-1 text-xs leading-relaxed obs-dim">
								This downloads every standard Agent, Secret Agent, and 00 Agent history entry, including older records
								and available YouTube proofs. Importing again safely skips times already added.
							</p>
							<p class="mt-2 text-xs leading-relaxed obs-dim">
								Use the name after <span class="font-mono">~</span> in your history URL, for example
								<span class="font-mono break-all">https://rankings.the-elite.net/~acheronfail/goldeneye/history</span>.
							</p>
						</div>
						<div class="flex flex-col gap-1">
							<label for="the-elite-username" class="font-mono text-xs obs-dim">The Elite username</label>
							<div class="flex items-center gap-2">
								<span class="font-mono text-sm obs-dim">~</span>
								<input
									id="the-elite-username"
									class="obs-input min-w-0 flex-1 px-3 py-2 font-mono"
									required
									bind:value={username}
									placeholder="username"
								/>
							</div>
						</div>
						{#if result}
							<p class="obs-alert-success rounded px-4 py-3 text-sm" role="status">
								Imported {result.imported}
								{result.imported === 1 ? 'time' : 'times'} with {result.videos} YouTube
								{result.videos === 1 ? ' video' : ' videos'}. {result.alreadyImported} already existed.
							</p>
						{/if}
						<div class="flex justify-end">
							<button
								type="submit"
								disabled={busy !== null}
								class="obs-button inline-flex items-center gap-2 obs-button-gold px-3 py-2 font-mono text-xs disabled:opacity-50"
							>
								{#if busy === 'elite'}
									<span
										class="size-3 animate-spin rounded-full border-2 border-current border-r-transparent"
										aria-hidden="true"
									></span>
									importing all times...
								{:else}
									import all times
								{/if}
							</button>
						</div>
					</form>
				{/if}

				{#if error}
					<div class="mt-4 rounded obs-alert-error px-4 py-3">
						<p class="text-sm font-semibold obs-alert-error-title">Could not add times</p>
						<p class="mt-1 font-mono text-xs obs-alert-error-body">{error}</p>
					</div>
				{/if}
			</div>
		</dialog>
	</div>
{/if}
