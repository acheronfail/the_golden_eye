<script lang="ts">
	import type { PluginUpdate } from '$lib/api';

	let {
		update,
		dismiss,
		openRelease
	}: {
		update: PluginUpdate;
		dismiss: () => void;
		openRelease: () => void | Promise<void>;
	} = $props();
	let laterButton = $state<HTMLButtonElement>();

	$effect(() => {
		queueMicrotask(() => laterButton?.focus());

		const onKeydown = (event: KeyboardEvent) => {
			if (event.key === 'Escape') dismiss();
		};
		window.addEventListener('keydown', onKeydown);
		return () => window.removeEventListener('keydown', onKeydown);
	});

	const open = async () => {
		await openRelease();
		dismiss();
	};
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center obs-overlay p-4" role="presentation">
	<div
		class="w-full max-w-md overflow-hidden rounded obs-dialog"
		role="dialog"
		aria-modal="true"
		aria-labelledby="manual-update-dialog-title"
		aria-describedby="manual-update-dialog-body"
	>
		<div class="obs-dialog-header px-4 py-3">
			<h2 id="manual-update-dialog-title" class="text-lg font-semibold obs-heading">Manual plugin update required</h2>
		</div>
		<div id="manual-update-dialog-body" class="grid gap-3 px-4 py-4 text-sm leading-6">
			<p>
				Version {update.latestVersion} uses a newer updater format and cannot be installed automatically by this version of
				the plugin.
			</p>
			<p>Open the release page, close OBS, and install the plugin package manually.</p>
			<p>Your Golden Eye settings and run history will be kept.</p>
		</div>
		<div class="flex justify-end gap-2 px-4 pb-4">
			<button bind:this={laterButton} type="button" class="obs-button px-4 py-2" onclick={dismiss}>Later</button>
			<button type="button" class="obs-button obs-button-gold px-4 py-2" onclick={open}>Open release page</button>
		</div>
	</div>
</div>
