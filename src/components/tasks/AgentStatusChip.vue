<template>
	<span
		v-if="enabled && (status || !hideWhenEmpty)"
		class="inline-flex h-[22px] min-w-[26px] shrink-0 items-center justify-center"
		data-testid="agent-status-slot"
	>
		<span
			v-if="status"
			class="inline-flex h-[22px] items-center gap-0.5 rounded-pill bg-surface-sunken px-1.5 text-2xs leading-none font-semibold"
			:class="[stateClass, status.stale && 'opacity-50']"
			:title="tooltip"
			:aria-label="ariaLabel"
			role="img"
			data-testid="agent-status-chip"
			:data-state="status.state"
			@click.stop
		>
			<span class="text-xs" aria-hidden="true">{{ glyph }}</span>
			<span v-if="status.count > 1" class="tabular-nums" aria-hidden="true">{{
				status.count
			}}</span>
		</span>
	</span>
</template>

<script>
	import {
		trackAgentStatus,
		useTaskAgentStatus,
	} from '@/composable/useAgentStatus';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { usePusher } from '@/composable/usePusher';
	import {
		STATE_GLYPHS,
		STATE_LABELS,
		statusTooltip,
	} from '@/utils/agentStatus';
	import { computed, defineComponent, onBeforeUnmount, watch } from 'vue';
	import { useStore } from 'vuex';

	const STATE_CLASSES = {
		working: 'text-tmgr-light-blue dark:text-sky-400',
		blocked: 'text-amber-600 dark:text-amber-400',
		waiting: 'text-amber-600 dark:text-amber-400',
		idle: 'text-ink-faint',
		done: 'text-status-done-fg',
		exited: 'text-ink-faint',
	};

	export default defineComponent({
		name: 'AgentStatusChip',
		props: {
			taskId: { type: Number, required: true },
			workspaceId: { type: Number, default: undefined },
			hideWhenEmpty: { type: Boolean, default: false },
		},
		setup(props) {
			const { isFeatureEnabled } = useFeatureToggles();
			const { subscribeToWorkspace, unsubscribeHandlerFromWorkspace } =
				usePusher();
			const store = useStore();
			const effectiveWorkspaceId = computed(
				() =>
					props.workspaceId || Number(store.getters.currentWorkspaceId) || null,
			);
			const enabled = computed(
				() => !!effectiveWorkspaceId.value && isFeatureEnabled('agent_work'),
			);
			const status = useTaskAgentStatus(
				() => (enabled.value ? effectiveWorkspaceId.value : null),
				() => props.taskId,
			);
			let release = null;
			watch(
				() => (enabled.value ? effectiveWorkspaceId.value : null),
				(workspaceId) => {
					if (release) release();
					release = workspaceId
						? trackAgentStatus(workspaceId, {
								subscribeToWorkspace,
								unsubscribeHandlerFromWorkspace,
							})
						: null;
				},
				{ immediate: true },
			);
			onBeforeUnmount(() => release && release());

			const tooltip = computed(() =>
				status.value ? statusTooltip(status.value, Date.now()) : '',
			);
			const glyph = computed(() => STATE_GLYPHS[status.value?.state] ?? '');
			const stateClass = computed(() => STATE_CLASSES[status.value?.state]);
			const ariaLabel = computed(() => {
				if (!status.value) return undefined;
				const label = `Agent ${STATE_LABELS[status.value.state].toLowerCase()}`;
				return status.value.count > 1
					? `${label}, ${status.value.count} sessions`
					: label;
			});

			return { enabled, status, tooltip, glyph, stateClass, ariaLabel };
		},
	});
</script>
