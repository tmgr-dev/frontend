<template>
	<p v-if="labels.length > 0" class="mgr-banner" role="alert">
		{{ $t('manager.pairing.blocked', { devices: labels.join(', ') }) }}
	</p>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'ManagerGateBlockersBanner',
		props: {
			labels: { type: Array as PropType<string[]>, required: true },
		},
	});
</script>
