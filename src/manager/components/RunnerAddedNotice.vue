<template>
	<div class="mgr-banner mgr-notice" role="status">
		<span>{{ $t('manager.pairing.added', { machine: label }) }}</span>
		<button type="button" class="btn small" @click="$emit('revoke')">
			{{ $t('manager.pairing.revoke') }}
		</button>
		<button
			type="button"
			class="ibd"
			:aria-label="$t('manager.pairing.dismiss')"
			@click="$emit('dismiss')"
		>
			&times;
		</button>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'ManagerRunnerAddedNotice',
		props: {
			label: { type: String, required: true },
		},
		emits: ['revoke', 'dismiss'],
	});
</script>
