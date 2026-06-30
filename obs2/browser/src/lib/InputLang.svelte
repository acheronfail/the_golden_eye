<script lang="ts">
	import { settings } from '$lib/settings.svelte';
</script>

<fieldset>
	<legend class="mb-2 font-semibold">Language:</legend>
	<div class="flex flex-col pl-4">
		<label class="mr-4">
			<input type="radio" name="lang" value="en" bind:group={settings.lang} />
			English
		</label>
		<label>
			<input type="radio" name="lang" value="jp" bind:group={settings.lang} />
			Japanese
		</label>
	</div>
</fieldset>
