<template>
	<section
		v-if="runner.enabled.value"
		class="flex flex-col gap-2"
		data-testid="task-agent-runner"
	>
		<div class="flex flex-wrap items-center gap-2">
			<span class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
				>Agent runner</span
			>
			<span class="flex-1" />
			<RunWithAgentButton :workspace-id="workspaceId" :task-id="taskId" />
		</div>
		<AgentSessionsPanel
			v-if="latest"
			:workspace-id="workspaceId"
			:task-id="taskId"
		/>
	</section>
</template>

<script lang="ts">
	import AgentSessionsPanel from '@/components/runner/AgentSessionsPanel.vue';
	import RunWithAgentButton from '@/components/runner/RunWithAgentButton.vue';
	import { useRunner } from '@/runner/store';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'TaskAgentRunner',
		components: { AgentSessionsPanel, RunWithAgentButton },
		props: {
			workspaceId: {
				type: [Number, String] as PropType<number | string | null | undefined>,
				default: null,
			},
			taskId: { type: [Number, String], required: true },
		},
		setup(props) {
			const runner = useRunner();
			const latest = computed(() =>
				props.workspaceId == null
					? null
					: runner.sessionForTask(
							Number(props.workspaceId),
							Number(props.taskId),
						),
			);
			return { runner, latest };
		},
	});
</script>
