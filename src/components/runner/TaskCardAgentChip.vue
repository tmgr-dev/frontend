<template>
	<AgentStateChip v-if="session" :state="session.state" />
</template>

<script lang="ts">
	import AgentStateChip from '@/components/runner/AgentStateChip.vue';
	import { useRunner } from '@/runner/store';
	import { computed, defineComponent, type PropType } from 'vue';

	type Id = number | string | null | undefined;

	export default defineComponent({
		name: 'TaskCardAgentChip',
		components: { AgentStateChip },
		props: {
			workspaceId: { type: [Number, String] as PropType<Id>, default: null },
			taskId: { type: [Number, String] as PropType<Id>, default: null },
		},
		setup(props) {
			const runner = useRunner();
			const session = computed(() =>
				runner.enabled.value &&
				props.workspaceId != null &&
				props.taskId != null
					? runner.sessionForTask(
							Number(props.workspaceId),
							Number(props.taskId),
					  )
					: null,
			);
			return { session };
		},
	});
</script>
