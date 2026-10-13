<template>
	<section
		v-if="runner.enabled.value"
		class="flex flex-col gap-2"
		data-testid="task-agent-runner"
	>
		<div class="flex flex-wrap items-center gap-2">
			<span class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
				>Agent runner</span
			>
			<span class="flex-1" />
			<RunWithAgentButton
				:workspace-id="workspaceId"
				:task-id="taskId"
				:task-title="taskTitle"
			/>
		</div>
		<AgentSessionsPanel
			v-if="latest"
			:workspace-id="workspaceId"
			:task-id="taskId"
		/>
	</section>
</template>

<script setup lang="ts">
	import { computed } from 'vue';
	import AgentSessionsPanel from '@/components/runner/AgentSessionsPanel.vue';
	import RunWithAgentButton from '@/components/runner/RunWithAgentButton.vue';
	import { useRunner } from '@/runner/store';

	const props = defineProps<{
		workspaceId: number | string | null | undefined;
		taskId: number | string;
		taskTitle: string;
	}>();

	const runner = useRunner();
	const latest = computed(() =>
		props.workspaceId == null
			? null
			: runner.sessionForTask(Number(props.workspaceId), Number(props.taskId)),
	);
</script>
