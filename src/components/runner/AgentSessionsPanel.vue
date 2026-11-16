<template>
	<div class="flex flex-col gap-2" data-testid="agent-sessions-panel">
		<p
			v-if="unavailable"
			class="rounded-md border border-line px-3 py-2 text-sm text-ink-subtle"
			role="status"
			data-testid="agent-sessions-error"
		>
			<span class="font-semibold text-destructive">{{
				$t('desktop.runner.sessions.error')
			}}</span>
			<CommandText :text="unavailable" />
		</p>
		<p
			v-if="!unavailable && !list.length"
			class="rounded-md border border-dashed border-line-strong px-3 py-3 text-sm text-ink-subtle"
			data-testid="agent-sessions-empty"
		>
			{{ $t('desktop.runner.sessions.empty') }}
		</p>
		<label
			v-if="!unavailable"
			class="flex items-center gap-2 text-xs text-ink-subtle"
			data-testid="agent-auto-switch"
		>
			<input
				type="checkbox"
				:checked="autoSwitch"
				@change="setAuto(($event.target as HTMLInputElement).checked)"
			/>
			{{ $t('desktop.runner.sessions.autoSwitch') }}
		</label>
		<ul
			v-if="!unavailable && list.length"
			class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
		>
			<li
				v-for="{ session, label } in rows"
				:key="session.id"
				class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2.5 text-sm"
				:data-session="session.id"
			>
				<span
					v-if="label"
					class="rounded border border-line px-1.5 text-xs font-medium text-ink-subtle"
					data-testid="agent-team-label"
					>{{ label }}</span
				>
				<AgentStateChip :state="session.state" />
				<span
					v-if="exitCodeLabel(session.state)"
					class="text-xs text-ink-subtle"
					data-testid="agent-exit-code"
					>{{ exitCodeLabel(session.state) }}</span
				>
				<span class="min-w-0 flex-1 truncate font-medium text-ink">{{
					session.name
				}}</span>
				<span
					v-if="session.limit"
					class="rounded border border-line px-1.5 text-xs font-medium text-destructive"
					data-testid="agent-limit"
					>{{ $t('desktop.runner.sessions.limitReached') }}</span
				>
				<span v-if="session.profile" class="text-xs text-ink-subtle">{{
					profileLabel(session.profile)
				}}</span>
				<span class="text-xs text-ink-subtle">{{
					formatAge(session.createdMs, now)
				}}</span>
				<Button
					variant="outline"
					size="sm"
					data-testid="agent-open-terminal"
					@click="openTerminal(session.id)"
				>
					{{ $t('desktop.runner.openTerminal') }}
				</Button>
				<Button
					v-if="canSwitch(session)"
					variant="outline"
					size="sm"
					data-testid="agent-switch"
					@click="switchAgent(session.id, false)"
				>
					{{ $t('desktop.runner.sessions.switchAgent') }}
				</Button>
				<Button
					v-if="session.state.kind !== 'exited'"
					variant="outline"
					size="sm"
					class="text-destructive"
					data-testid="agent-stop"
					@click="stop(session.id)"
				>
					{{ $t('desktop.runner.sessions.stop') }}
				</Button>
			</li>
		</ul>
		<p
			v-if="notice"
			class="text-xs text-ink-subtle"
			role="status"
			data-testid="agent-switch-notice"
		>
			{{ notice }}
		</p>
		<p v-if="actionError" class="text-xs text-destructive" role="alert">
			<CommandText :text="actionError" />
		</p>
	</div>
</template>

<script lang="ts">
	import CommandText from '@/components/general/CommandText.vue';
	import AgentStateChip from '@/components/runner/AgentStateChip.vue';
	import { Button } from '@/components/ui/button';
	import {
		runnerAutoSwitch,
		runnerHalt,
		runnerOpenTerminal,
		runnerSetAutoSwitch,
		runnerSwitchAgent,
		type RunnerSession,
	} from '@/runner/api';
	import { describeRunnerError } from '@/runner/errors';
	import { formatAge } from '@/runner/format';
	import { absentTooltip } from '@/runner/runButton';
	import { exitCodeLabel } from '@/runner/states';
	import { statusDetail } from '@/runner/statusLine';
	import { useRunner } from '@/runner/store';
	import { groupTeams } from '@/runner/teams';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
		type PropType,
	} from 'vue';

	type Id = number | string | null | undefined;

	const profileLabels: Record<string, string> = {
		'claude-code': 'Claude Code',
		codex: 'Codex',
	};

	export default defineComponent({
		name: 'AgentSessionsPanel',
		components: { AgentStateChip, Button, CommandText },
		props: {
			workspaceId: { type: [Number, String] as PropType<Id>, default: null },
			taskId: { type: [Number, String] as PropType<Id>, default: null },
		},
		setup(props) {
			const runner = useRunner();
			const now = ref(Date.now());
			const actionError = ref('');
			const notice = ref('');
			const autoSwitch = ref(false);
			const tried = new Set<string>();
			let timer: ReturnType<typeof setInterval> | undefined;

			onMounted(() => {
				timer = setInterval(() => (now.value = Date.now()), 30_000);
				runnerAutoSwitch()
					.then((on) => (autoSwitch.value = on))
					.catch(() => undefined);
			});
			onBeforeUnmount(() => clearInterval(timer));

			const list = computed(() => {
				if (props.workspaceId != null && props.taskId != null)
					return runner.sessionsForTask(
						Number(props.workspaceId),
						Number(props.taskId),
					);
				return [...runner.sessions.value].sort(
					(a, b) => b.createdMs - a.createdMs,
				);
			});

			const rows = computed(() => groupTeams(list.value));

			const unavailable = computed(() => {
				const state = runner.status.value?.state;
				if (state === 'incompatible')
					return statusDetail(runner.status.value) ?? '';
				if (state && state !== 'ready') return absentTooltip();
				return '';
			});

			const guarded = async (
				action: () => Promise<void>,
				kind: 'stop' | 'open' | 'switch',
			) => {
				actionError.value = '';
				try {
					await action();
				} catch (error) {
					const view = describeRunnerError(error, kind);
					if (!view.cancelled) actionError.value = view.text;
				}
			};

			const canSwitch = (s: RunnerSession) =>
				!!s.task &&
				!s.team &&
				(s.profile === 'claude-code' || s.profile === 'codex');

			const switchAgent = (id: string, auto: boolean) =>
				guarded(async () => {
					notice.value = '';
					const result = await runnerSwitchAgent(id, null, auto);
					notice.value = [
						result.session.name,
						result.safeModeNote,
						result.warning,
					]
						.filter(Boolean)
						.join(' ');
				}, 'switch');

			const setAuto = (on: boolean) => {
				autoSwitch.value = on;
				runnerSetAutoSwitch(on).catch(() => (autoSwitch.value = !on));
			};

			watch(
				() => [autoSwitch.value, runner.sessions.value] as const,
				([on, sessions]) => {
					if (!on) return;
					for (const s of sessions) {
						if (!s.limit || !canSwitch(s) || tried.has(s.id)) continue;
						if (sessions.some((o) => o.handoffFrom === s.id)) continue;
						tried.add(s.id);
						switchAgent(s.id, true);
					}
				},
				{ deep: true },
			);

			return {
				canSwitch,
				switchAgent,
				setAuto,
				autoSwitch,
				notice,
				list,
				rows,
				unavailable,
				now,
				actionError,
				formatAge,
				exitCodeLabel,
				profileLabel: (id: string) => profileLabels[id] ?? id,
				openTerminal: (id: string) =>
					guarded(() => runnerOpenTerminal(id), 'open'),
				stop: (id: string) => guarded(() => runnerHalt(id), 'stop'),
			};
		},
	});
</script>
