<template>
	<AgentStateChip v-if="session" :state="session.state" />
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import AgentStateChip from '@/components/runner/AgentStateChip.vue';
	import { useRunner } from '@/runner/store';

	const props = defineProps<{
		workspaceId: number | string | null | undefined;
		taskId: number | string | null | undefined;
	}>();

	const runner = useRunner();
	const session = computed(() =>
		runner.enabled.value && props.workspaceId != null && props.taskId != null
			? runner.sessionForTask(Number(props.workspaceId), Number(props.taskId))
			: null,
	);
</script>
