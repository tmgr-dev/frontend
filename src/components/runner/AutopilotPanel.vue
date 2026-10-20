<template>
	<SettingsSection
		title="Autopilot"
		description="Live status of the local autopilot, read from your runner."
	>
		<div class="flex flex-col gap-4" data-testid="autopilot-panel">
			<p
				v-if="problem"
				class="rounded-md border border-line px-3 py-2 text-sm text-ink-subtle"
				role="status"
				data-testid="autopilot-problem"
			>
				{{ problem }}
			</p>
			<div
				v-else-if="view?.empty"
				class="rounded-md border border-dashed border-line-strong px-3 py-3 text-sm text-ink-subtle"
				data-testid="autopilot-empty"
			>
				{{ view.emptyText }} Start it with
				<CommandChip :command="view.emptyCommand" />
			</div>
			<template v-else-if="view && view.header">
				<div class="flex flex-wrap items-center gap-x-3 gap-y-2">
					<span class="text-sm font-semibold text-ink">{{
						view.header.title
					}}</span>
					<span
						class="rounded-full border border-line px-2 py-0.5 text-xs font-medium text-ink"
						data-testid="autopilot-state"
						>{{ view.header.word }}</span
					>
					<span class="text-xs text-ink-subtle">
						<template v-if="view.header.startedAt"
							>started {{ view.header.startedAt }}</template
						>
						<template v-if="view.header.runningFor">
							· running for {{ view.header.runningFor }}</template
						>
						<template v-if="view.header.rule">
							· rule {{ view.header.rule }}</template
						>
						<template v-if="view.header.mode">
							· mode {{ view.header.mode }}</template
						>
						<template v-if="view.header.teams">
							· teams {{ view.header.teams }}</template
						>
						<template v-if="view.header.agents">
							· agents {{ view.header.agents }}</template
						>
					</span>
					<span class="flex-1" />
					<Button
						v-if="view.buttons.pause"
						variant="outline"
						size="sm"
						:disabled="busy"
						data-testid="autopilot-pause"
						@click="control('pause')"
						>Pause</Button
					>
					<Button
						v-if="view.buttons.resume"
						variant="outline"
						size="sm"
						:disabled="busy"
						data-testid="autopilot-resume"
						@click="control('resume')"
						>Resume</Button
					>
					<Button
						v-if="view.buttons.stop"
						variant="outline"
						size="sm"
						class="text-destructive"
						:disabled="busy"
						data-testid="autopilot-stop"
						@click="control('stop')"
						>Stop</Button
					>
				</div>
				<p v-if="view.header.note" class="text-xs text-ink-subtle">
					{{ view.header.note }}
				</p>
				<p v-if="view.header.stopReason" class="text-xs text-ink-subtle">
					stopped: {{ view.header.stopReason }}
				</p>
				<p
					v-if="view.header.stale"
					class="text-xs text-amber-700 dark:text-amber-400"
				>
					No update from the autopilot for a while.
				</p>
				<p
					v-if="view.header.needsUpdate"
					class="text-xs text-amber-700 dark:text-amber-400"
				>
					This status comes from a newer autopilot. Update the app.
				</p>
				<p v-if="actionError" class="text-xs text-destructive" role="alert">
					{{ actionError }}
				</p>

				<div class="grid grid-cols-2 gap-2 sm:grid-cols-4 lg:grid-cols-7">
					<button
						v-for="c in view.counters"
						:key="c.key"
						type="button"
						class="rounded-lg border border-border bg-card p-3 text-left"
						:class="c.items ? 'hover:bg-muted' : 'cursor-default'"
						:disabled="!c.items"
						:data-testid="`autopilot-counter-${c.key}`"
						@click="open = open === c.key ? null : c.key"
					>
						<span class="block text-2xl font-semibold tabular-nums text-ink">{{
							c.count
						}}</span>
						<span class="block text-xs text-ink-subtle">{{ c.label }}</span>
					</button>
				</div>
				<ul
					v-if="openCounter && openCounter.items"
					class="divide-y divide-line rounded-card border border-line bg-surface text-sm"
					data-testid="autopilot-counter-list"
				>
					<li
						v-if="!openCounter.items.length"
						class="px-3 py-2 text-ink-subtle"
					>
						Nothing in this list yet.
					</li>
					<li
						v-for="item in openCounter.items"
						:key="item.task + (item.url || '')"
						class="flex flex-wrap items-center gap-x-3 px-3 py-2"
					>
						<span class="font-medium text-ink">{{ item.task }}</span>
						<span class="min-w-0 flex-1 truncate text-ink-subtle">{{
							item.title || item.reason || ''
						}}</span>
						<Button
							v-if="item.url"
							variant="outline"
							size="sm"
							@click="openUrl(item.url)"
							>Open PR</Button
						>
					</li>
				</ul>

				<div v-if="view.runs.length" class="flex flex-col gap-2">
					<h3
						class="text-xs font-semibold uppercase tracking-wide text-ink-subtle"
					>
						Active runs
					</h3>
					<ul
						class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
					>
						<li
							v-for="r in view.runs"
							:key="r.task"
							class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2.5 text-sm"
							:data-testid="`autopilot-run-${r.task}`"
						>
							<span class="font-medium text-ink">{{ r.task }}</span>
							<span class="min-w-0 flex-1 truncate text-ink">{{
								r.title
							}}</span>
							<span class="text-xs text-ink-subtle">{{ r.mode }}</span>
							<span class="text-xs tabular-nums text-ink-subtle">{{
								r.duration
							}}</span>
							<span v-if="r.repos.length" class="text-xs text-ink-subtle">{{
								r.repos.join(', ')
							}}</span>
							<span class="text-xs text-ink-faint">{{ r.step }}</span>
							<span v-if="r.agents" class="text-xs text-ink-subtle">{{
								r.agents
							}}</span>
							<Button
								variant="outline"
								size="sm"
								:disabled="!r.workspaceCode || r.taskId == null"
								:title="
									r.workspaceCode
										? ''
										: 'Workspace of this task is not reported'
								"
								data-testid="autopilot-open-task"
								@click="openTask(r)"
								>Open task</Button
							>
							<Button
								variant="outline"
								size="sm"
								:disabled="!sessionFor(r)"
								:title="
									sessionFor(r)
										? ''
										: 'No live agent session found for this task'
								"
								data-testid="autopilot-open-session"
								@click="openSession(r)"
								>Open session</Button
							>
							<Button
								v-if="r.prUrl"
								variant="outline"
								size="sm"
								@click="openUrl(r.prUrl)"
								>Open PR</Button
							>
						</li>
					</ul>
				</div>

				<div
					v-if="view.limits.length || view.limitsNote"
					class="flex flex-col gap-3"
				>
					<h3
						class="text-xs font-semibold uppercase tracking-wide text-ink-subtle"
					>
						Limits
					</h3>
					<p v-if="view.limitsNote" class="text-sm text-ink-subtle">
						{{ view.limitsNote }}
					</p>
					<div
						v-for="l in view.limits"
						:key="l.label"
						data-testid="autopilot-limit"
					>
						<p class="text-sm text-ink">{{ l.text }}</p>
						<div
							v-if="l.usedPct != null"
							class="relative mt-1 h-2 rounded-full bg-muted"
							role="progressbar"
							aria-valuemin="0"
							aria-valuemax="100"
							:aria-valuenow="Math.round(l.usedPct)"
						>
							<div
								class="h-full rounded-full"
								:class="barClass(l)"
								:style="{ width: Math.min(100, l.usedPct) + '%' }"
							/>
							<span
								v-if="l.soft != null"
								class="absolute -top-0.5 h-3 w-px bg-ink-subtle"
								:style="{ left: Math.min(100, l.soft) + '%' }"
							/>
							<span
								v-if="l.hard != null"
								class="absolute -top-0.5 h-3 w-px bg-destructive"
								:style="{ left: Math.min(100, l.hard) + '%' }"
							/>
						</div>
					</div>
					<p
						v-if="view.nextStop"
						class="text-xs text-ink-subtle"
						data-testid="autopilot-next-stop"
					>
						{{ view.nextStop }}
					</p>
				</div>

				<p
					v-if="view.cost"
					class="text-sm text-ink"
					data-testid="autopilot-cost"
				>
					Cost: subscription {{ view.cost.subscription }}
					<template v-if="view.cost.basis">({{ view.cost.basis }})</template>
					· {{ view.cost.tokens.toLocaleString('en-US') }} tokens · API
					{{ view.cost.api }}
				</p>
				<p
					v-if="view.unconfirmedNew"
					class="text-sm text-amber-700 dark:text-amber-400"
				>
					{{ view.unconfirmedNew }} new tasks need your confirmation
				</p>

				<div v-if="view.events.length" class="flex flex-col gap-1">
					<h3
						class="text-xs font-semibold uppercase tracking-wide text-ink-subtle"
					>
						Events
					</h3>
					<ul
						class="max-h-64 divide-y divide-line overflow-auto rounded-card border border-line bg-surface text-sm"
						data-testid="autopilot-events"
					>
						<li
							v-for="(e, i) in view.events"
							:key="i"
							class="flex flex-wrap gap-x-3 px-3 py-1.5"
						>
							<span class="tabular-nums text-ink-subtle">{{ e.at }}</span>
							<span class="font-medium text-ink">{{ e.kind }}</span>
							<a
								v-if="eventTarget(e)"
								href="#"
								class="text-ink underline"
								@click.prevent="followEvent(e)"
								>{{ e.task }}</a
							>
							<span v-else-if="e.task" class="text-ink">{{ e.task }}</span>
							<span class="min-w-0 flex-1 text-ink-subtle">{{ e.text }}</span>
						</li>
					</ul>
				</div>
			</template>
		</div>
	</SettingsSection>
</template>

<script lang="ts">
	import type {
		AutopilotView,
		EventView,
		LimitView,
		RunView,
	} from '@/autopilot/types';
	import { buildView, safeHttpsUrl } from '@/autopilot/view';
	import CommandChip from '@/components/general/CommandChip.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import {
		runnerAutopilotControl,
		runnerAutopilotStatus,
		runnerOpenTerminal,
		type RunnerAutopilotCommand,
	} from '@/runner/api';
	import { describeRunnerError } from '@/runner/errors';
	import { useRunner } from '@/runner/store';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';
	import { useRouter } from 'vue-router';

	const POLL_MS = 2000;
	const SLOW_MS = 10000;

	export default defineComponent({
		name: 'AutopilotPanel',
		components: { Button, CommandChip, SettingsSection },
		setup() {
			const router = useRouter();
			const runner = useRunner();
			const view = ref<AutopilotView | null>(null);
			const problem = ref('');
			const actionError = ref('');
			const busy = ref(false);
			const open = ref<string | null>(null);
			const now = ref(Date.now());
			let raw: Parameters<typeof buildView>[0] | null = null;
			let disposed = false;
			let timer: ReturnType<typeof setTimeout> | undefined;
			let tick: ReturnType<typeof setInterval> | undefined;

			const rebuild = () => {
				if (raw) view.value = buildView(raw, now.value);
			};

			const poll = async () => {
				let delay = POLL_MS;
				if (runner.status.value && runner.status.value.state !== 'ready') {
					problem.value =
						'The TMGR runner is not running. Start it to see autopilot here.';
					delay = SLOW_MS;
				} else if (
					runner.status.value &&
					!runner.status.value.caps.includes('autopilot.v1')
				) {
					problem.value = 'Update the TMGR runner to see autopilot here.';
					delay = SLOW_MS;
				} else if (!document.hidden) {
					try {
						raw = await runnerAutopilotStatus();
						problem.value = '';
						rebuild();
						if (!raw.running) delay = SLOW_MS;
					} catch (error) {
						problem.value = describeRunnerError(error, 'open').text;
						delay = SLOW_MS;
					}
				}
				if (!disposed) timer = setTimeout(poll, delay);
			};

			const onVisible = () => {
				if (!document.hidden && !disposed) {
					clearTimeout(timer);
					void poll();
				}
			};

			onMounted(() => {
				document.addEventListener('visibilitychange', onVisible);
				void poll();
				tick = setInterval(() => {
					now.value = Date.now();
					rebuild();
				}, 1000);
			});
			onBeforeUnmount(() => {
				disposed = true;
				document.removeEventListener('visibilitychange', onVisible);
				clearTimeout(timer);
				clearInterval(tick);
			});

			const control = async (command: RunnerAutopilotCommand) => {
				actionError.value = '';
				busy.value = true;
				try {
					await runnerAutopilotControl(command);
					try {
						raw = await runnerAutopilotStatus();
						rebuild();
					} catch {}
				} catch (error) {
					const e = describeRunnerError(error, 'stop');
					if (!e.cancelled) actionError.value = e.text;
				} finally {
					busy.value = false;
				}
			};

			const sessionFor = (r: RunView) => {
				if (r.sessionId)
					return (
						runner.sessions.value.find((s) => s.id === r.sessionId) ?? null
					);
				if (r.taskId == null) return null;
				const live = runner.sessions.value.filter(
					(s) => s.task?.taskId === r.taskId && s.state.kind !== 'exited',
				);
				return live.length === 1 ? live[0] : null;
			};

			const openSession = async (r: RunView) => {
				const s = sessionFor(r);
				if (!s) return;
				actionError.value = '';
				try {
					await runnerOpenTerminal(s.id);
				} catch (error) {
					const e = describeRunnerError(error, 'open');
					if (!e.cancelled) actionError.value = e.text;
				}
			};

			const openTask = (r: RunView) => {
				if (r.workspaceCode && r.taskId != null)
					void router.push(`/${r.workspaceCode}/tasks/${r.taskId}`);
			};

			const openUrl = (url: string) => {
				const safe = safeHttpsUrl(url);
				if (safe) window.open(safe, '_blank', 'noopener,noreferrer');
			};

			const eventTarget = (e: EventView) =>
				!!(e.url && safeHttpsUrl(e.url)) || !!findRun(e.task);
			const findRun = (task: string | null) =>
				task ? view.value?.runs.find((r) => r.task === task) : undefined;
			const followEvent = (e: EventView) => {
				if (e.url) return openUrl(e.url);
				const r = findRun(e.task);
				if (r) openTask(r);
			};

			const openCounter = computed(
				() => view.value?.counters.find((c) => c.key === open.value) ?? null,
			);
			const barClass = (l: LimitView) =>
				l.hard != null && l.usedPct! >= l.hard
					? 'bg-red-500'
					: l.soft != null && l.usedPct! >= l.soft
					? 'bg-amber-500'
					: 'bg-emerald-500';

			return {
				view,
				problem,
				actionError,
				busy,
				open,
				openCounter,
				control,
				sessionFor,
				openSession,
				openTask,
				openUrl,
				eventTarget,
				followEvent,
				barClass,
			};
		},
	});
</script>
