<script lang="ts">
	import { settings } from '$lib/settings.svelte';
	import * as obs from '$lib/obs';

	let testSuccess = $state(false);

	const testConnection = async () => {
		testSuccess = await obs.testConnection();
	};
</script>

<div class="flex min-h-screen items-center justify-center p-4">
	<div class="flex flex-col gap-4">
		<div class="flex flex-col gap-2">
			<h1 class="text-2xl font-bold">OBS Connection</h1>
			<p>First, let's get OBS connected and ready.</p>
		</div>

		<div class="flex flex-col gap-2">
			<label for="obs-url" class="font-medium">OBS WebSocket URL</label>
			<input
				id="obs-url"
				type="text"
				placeholder="ws://localhost:4455"
				class="input input-bordered w-full"
				bind:value={settings.obsUrl}
			/>
		</div>

		<div class="flex flex-col gap-2">
			<label for="obs-password" class="font-medium">OBS WebSocket Password</label>
			<input
				id="obs-password"
				type="password"
				placeholder="Your OBS password"
				class="input input-bordered w-full"
				bind:value={settings.obsPassword}
			/>
		</div>

		<button class="btn btn-primary w-full" onclick={testConnection}>Test connection to OBS</button>
		{#if testSuccess}
			<div class="alert alert-success mt-4">
				<span>Successfully connected to OBS!</span>
			</div>
		{/if}
	</div>
</div>
