<template>
	<div class="flex flex-col gap-2" data-testid="agent-sessions-panel">
		<p
			v-if="unavailable"
			class="rounded-md border border-line px-3 py-2 text-sm text-ink-subtle"
			role="status"
			data-testid="agent-sessions-error"
		>
			<span class="font-semibold text-destructive">error:</span>
			{{ unavailable }}
		</p>
		<p
			v-else-if="!list.length"
			class="rounded-md border border-dashed border-line-strong px-3 py-3 text-sm text-ink-subtle"
			data-testid="agent-sessions-empty"
		>
			No agents running. Start one with Run with agent on a task.
		</p>
		<ul
			v-else
			class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
		>
			<li
				v-for="session in list"
				:key="session.id"
				class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2.5 text-sm"
				:data-session="session.id"
			>
				<AgentStateChip :state="session.state" />
				<span class="min-w-0 flex-1 truncate font-medium text-ink">{{
					session.name
				}}</span>
				<span v-if="session.profile" class="text-xs text-ink-subtle">{{
					profileLabel(session.profile)
				}}</span>
				<span class="text-xs text-ink-faint">{{
					formatAge(session.createdMs, now)
				}}</span>
				<Button
					variant="outline"
					size="sm"
					data-testid="agent-open-terminal"
					@click="openTerminal(session.id)"
				>
					Open terminal
				</Button>
				<Button
					v-if="session.state.kind !== 'exited'"
					variant="outline"
					size="sm"
					class="text-destructive"
					data-testid="agent-stop"
					@click="stop(session.id)"
				>
					Stop
				</Button>
			</li>
		</ul>
		<p v-if="actionError" class="text-xs text-destructive" role="alert">
			{{ actionError }}
		</p>
	</div>
</template>

<script setup lang="ts">
	import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
	import AgentStateChip from '@/components/runner/AgentStateChip.vue';
	import { Button } from '@/components/ui/button';
	import { runnerHalt, runnerOpenTerminal } from '@/runner/api';
	import { formatAge } from '@/runner/format';
	import { ABSENT_TOOLTIP } from '@/runner/runButton';
	import { useRunner } from '@/runner/store';

	const props = defineProps<{
		workspaceId?: number | string | null;
		taskId?: number | string | null;
	}>();

	const profileLabels: Record<string, string> = {
		'claude-code': 'Claude Code',
		codex: 'Codex',
	};
	const profileLabel = (id: string) => profileLabels[id] ?? id;

	const runner = useRunner();
	const now = ref(Date.now());
	const actionError = ref('');
	let timer: ReturnType<typeof setInterval> | undefined;

	onMounted(() => {
		timer = setInterval(() => (now.value = Date.now()), 30_000);
	});
	onBeforeUnmount(() => clearInterval(timer));

	const list = computed(() => {
		if (props.workspaceId != null && props.taskId != null)
			return runner.sessionsForTask(
				Number(props.workspaceId),
				Number(props.taskId),
			);
		return [...runner.sessions.value].sort((a, b) => b.createdMs - a.createdMs);
	});

	const unavailable = computed(() => {
		const state = runner.status.value?.state;
		if (state === 'incompatible')
			return 'The TMGR runner version is not compatible with this app. Update the CLI.';
		if (state && state !== 'ready') return ABSENT_TOOLTIP;
		return '';
	});

	const guarded = async (action: () => Promise<void>) => {
		actionError.value = '';
		try {
			await action();
		} catch (error) {
			const text = String((error as any)?.message ?? error);
			if (text !== 'cancelled') actionError.value = text;
		}
	};

	const openTerminal = (id: string) => guarded(() => runnerOpenTerminal(id));
	const stop = (id: string) => guarded(() => runnerHalt(id));
</script>
