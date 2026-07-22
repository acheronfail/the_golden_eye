<script lang="ts">
	let { dismiss }: { dismiss: () => void } = $props();
	let dismissButton = $state<HTMLButtonElement>();

	$effect(() => {
		queueMicrotask(() => dismissButton?.focus());
	});
</script>

<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
	<div
		class="obs-dialog w-full max-w-md overflow-hidden rounded"
		role="dialog"
		aria-modal="true"
		aria-labelledby="welcome-dialog-title"
		aria-describedby="welcome-dialog-body"
	>
		<div class="obs-dialog-header px-4 py-3">
			<h2 id="welcome-dialog-title" class="obs-heading text-lg font-semibold">Welcome to The Golden Eye</h2>
		</div>
		<div id="welcome-dialog-body" class="flex flex-col gap-4 px-4 py-4 text-sm leading-6">
			<p>
				This plugin helps save clips from GoldenEye 007 speedruns by watching your capture and managing replay-buffer
				saves around runs.
			</p>
			<p class="obs-alert-warning obs-alert-warning-body rounded px-3 py-2">
				Do not rely completely on this plugin as your only copy. You are strongly recommended to stream or record your
				gameplay somewhere reliable, such as YouTube or Twitch, so a missed detection or local recording issue does not
				cost you the run.
			</p>
		</div>
		<div class="flex justify-end gap-2 px-4 pb-4">
			<button bind:this={dismissButton} type="button" class="obs-button obs-button-gold px-4 py-2" onclick={dismiss}>
				I understand
			</button>
		</div>
	</div>
</div>
