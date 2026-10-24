<template>
	<section
		v-if="desktop && runs.length"
		class="flex flex-col gap-1"
		data-tour="task-form.agent-work"
	>
		<span class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
			>Agent work</span
		>
		<button
			type="button"
			class="flex items-center justify-between gap-2 rounded-card border border-line bg-surface px-3 py-2 text-left text-xs text-ink hover:bg-surface-hover"
			data-testid="agent-work-block"
			data-tour="task-form.agent-totals"
			@click="detailsOpen = true"
		>
			<span class="min-w-0 truncate">{{ blockLine }}</span>
			<span class="shrink-0 text-ink-subtle">Details ▸</span>
		</button>
		<AgentWorkDetails
			v-model:open="detailsOpen"
			:task-id="taskId"
			:workspace-id="workspaceId"
			:workspace-code="workspaceCode"
			:runs="runs"
			:totals="totals"
			:cost="cost"
			:cost-state="costState"
		/>
	</section>
	<section
		v-else-if="runs.length"
		class="flex flex-col gap-2"
		data-tour="task-form.agent-work"
	>
		<div class="flex items-baseline justify-between gap-2">
			<span class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
				>Agent work</span
			>
			<span class="text-2xs text-ink-faint" data-tour="task-form.agent-totals">
				Agents {{ formatWorkDuration(totals.agentSeconds) }} · You
				{{ formatWorkDuration(totals.humanSeconds) }}
			</span>
		</div>

		<ul
			class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
		>
			<li
				v-for="run in runs"
				:key="run.id"
				class="flex flex-col gap-1.5 px-3 py-2.5"
			>
				<div class="flex flex-wrap items-center gap-2 text-xs">
					<span class="font-semibold text-ink">{{
						agentLabel(run.agent)
					}}</span>
					<span v-if="run.model" class="text-ink-faint">{{ run.model }}</span>
					<span
						:class="[
							'rounded-pill px-2 py-0.5 text-2xs font-semibold',
							STATUS_TONE[run.status],
						]"
					>
						<span
							v-if="run.status === 'running'"
							class="mr-1 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-current align-middle"
						/>{{ run.status }}
					</span>
					<span class="ml-auto font-mono text-ink-subtle">
						{{ formatWorkDuration(liveSeconds(run, now, receivedAt[run.id])) }}
					</span>
				</div>

				<p
					v-if="run.summary"
					class="whitespace-pre-line text-sm text-ink"
					data-selectable
				>
					{{ run.summary }}
				</p>

				<div
					v-if="run.branch || run.pr_url || run.commits.length || run.tests"
					class="flex flex-wrap items-center gap-x-3 gap-y-1 text-2xs text-ink-subtle"
				>
					<span v-if="run.branch" class="font-mono" data-selectable>{{
						run.branch
					}}</span>
					<a
						v-if="isHttpUrl(run.pr_url)"
						:href="run.pr_url"
						target="_blank"
						rel="noopener noreferrer"
						class="text-brand hover:underline"
						>Pull request</a
					>
					<button
						v-if="run.commits.length"
						type="button"
						class="hover:text-ink"
						@click="toggle(run.id)"
					>
						{{ run.commits.length }}
						{{ run.commits.length === 1 ? 'commit' : 'commits' }}
						{{ expanded[run.id] ? '▾' : '▸' }}
					</button>
					<span v-if="run.tests">
						tests
						<span class="text-emerald-600 dark:text-emerald-400"
							>{{ run.tests.passed ?? 0 }} passed</span
						><template v-if="run.tests.failed"
							>,
							<span class="text-red-600 dark:text-red-400"
								>{{ run.tests.failed }} failed</span
							></template
						>
					</span>
					<span class="ml-auto text-ink-faint">{{
						startedLabel(run.started_at)
					}}</span>
				</div>

				<ul
					v-if="expanded[run.id]"
					class="flex flex-col gap-0.5 font-mono text-2xs text-ink-subtle"
					data-selectable
				>
					<li v-for="commit in run.commits" :key="commit.sha">
						<span class="text-ink">{{ (commit.sha || '').slice(0, 7) }}</span>
						{{ commit.message }}
					</li>
				</ul>
			</li>
		</ul>
	</section>
	<section
		v-else-if="showEmptyState"
		class="rounded-card border border-dashed border-line-strong px-3 py-3 text-center text-xs text-ink-subtle"
		data-tour="task-form.agent-work"
	>
		No agent runs yet. Runs from your MCP agent appear here.
	</section>
</template>

<script>
	import { getAgentWork } from '@/actions/tmgr/agentWork';
	import { getTaskCost } from '@/actions/tmgr/taskCost';
	import AgentWorkDetails from '@/components/tasks/AgentWorkDetails.vue';
	import { usePusher } from '@/composable/usePusher';
	import store from '@/store';
	import {
		agentRunEvent,
		showAgentWorkEmptyState,
	} from '@/tours/agentRunEvents';
	import { tourBus } from '@/tours/events';
	import { useTours } from '@/tours/useTours';
	import { readTaskUsage } from '@/usage/loadTaskUsage';
	import { blockSummary, prLabel, pricedCost } from '@/usage/taskModel';
	import {
		agentLabel,
		formatWorkDuration,
		isHttpUrl,
		liveSeconds,
		liveTotals,
		mergeSnapshot,
		upsertRun,
	} from '@/utils/agentWork';
	import { isDesktopApp } from '@/utils/desktop';
	import { formatDistanceToNow } from 'date-fns';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';

	const STATUS_TONE = {
		running: 'bg-sky-100 text-sky-700 dark:bg-sky-900/40 dark:text-sky-300',
		succeeded:
			'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300',
		failed: 'bg-red-100 text-red-700 dark:bg-red-900/40 dark:text-red-300',
		cancelled: 'bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300',
		abandoned:
			'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300',
	};

	export default defineComponent({
		name: 'TaskAgentWork',
		components: { AgentWorkDetails },
		props: {
			taskId: { type: Number, required: true },
			workspaceId: { type: Number, default: undefined },
		},
		setup(props) {
			const {
				subscribeToWorkspace,
				subscribeToUser,
				unsubscribeHandler,
				unsubscribeHandlerFromWorkspace,
			} = usePusher();
			const { active } = useTours();
			const runs = ref([]);
			const loaded = ref(false);
			const serverTotals = ref({
				agent_seconds: 0,
				human_seconds: 0,
				human_timer_running: false,
			});
			const totalsReceivedAt = ref(Date.now());
			const receivedAt = reactive({});
			const now = ref(Date.now());
			const expanded = reactive({});
			const desktop = isDesktopApp();
			const detailsOpen = ref(false);
			const cost = ref(null);
			const costState = ref('loading');
			const localCost = ref(null);
			const workspaceCode = computed(
				() => store.getters.workspaceById?.(props.workspaceId)?.code ?? '',
			);
			let request = 0;
			let ticker = null;
			let subscription = null;
			let subscribedWorkspace = null;
			let timerSubscription = null;
			let timerUserId = null;

			const markReceived = (list) => {
				const at = Date.now();
				list.forEach((run) => {
					receivedAt[run.id] = at;
				});
			};

			let usageReadFor = null;

			const loadCosts = async (list, current) => {
				const readUsage = usageReadFor !== props.taskId;
				const [fetchedCost, local] = await Promise.all([
					getTaskCost(props.taskId),
					readUsage
						? readTaskUsage({
								taskId: props.taskId,
								workspaceId: props.workspaceId,
								workspaceCode: workspaceCode.value,
								ownRuns: list,
							})
								.then((r) => r.usage)
								.catch(() => null)
						: Promise.resolve(undefined),
				]);
				if (current !== request) return;
				cost.value = fetchedCost;
				costState.value = fetchedCost ? 'ready' : 'failed';
				if (local !== undefined) {
					localCost.value = pricedCost(local?.aggregate);
					if (local) usageReadFor = props.taskId;
				}
			};

			const load = async () => {
				const current = ++request;
				try {
					const overview = await getAgentWork(props.taskId);
					if (current !== request) return;
					markReceived(overview.runs);
					runs.value = mergeSnapshot(runs.value, overview.runs);
					serverTotals.value = overview.totals;
					totalsReceivedAt.value = Date.now();
					loaded.value = true;
					if (desktop && overview.runs.length)
						loadCosts(overview.runs, current);
				} catch (error) {
					console.error('Failed to load agent work', error);
				}
			};

			const hasRunning = computed(
				() =>
					serverTotals.value.human_timer_running ||
					runs.value.some((run) => run.status === 'running'),
			);

			watch(
				hasRunning,
				(running) => {
					clearInterval(ticker);
					ticker = running
						? setInterval(() => {
								now.value = Date.now();
							}, 1000)
						: null;
				},
				{ immediate: true },
			);

			const subscribe = (workspaceId) => {
				if (subscription && subscribedWorkspace) {
					unsubscribeHandlerFromWorkspace(subscribedWorkspace, subscription);
				}
				subscription = null;
				subscribedWorkspace = workspaceId;
				if (!workspaceId) return;
				subscription = subscribeToWorkspace(workspaceId, {
					onAgentWorkChanged: (run) => {
						if (run?.task_id === props.taskId) {
							const event = agentRunEvent(runs.value, run, loaded.value);
							markReceived([run]);
							runs.value = upsertRun(runs.value, run);
							if (event) tourBus.emit(event, { task_id: props.taskId });
						}
					},
				});
			};

			watch(() => props.workspaceId, subscribe, { immediate: true });

			const followTimer = (userId) => {
				if (timerSubscription && timerUserId) {
					unsubscribeHandler(`App.User.${timerUserId}`, timerSubscription);
				}
				timerSubscription = null;
				timerUserId = userId || null;
				if (!userId) return;
				const reloadIfThisTask = (task) => {
					if (task?.id !== props.taskId) return;
					const sameWorkspace =
						task.workspace_id == null ||
						props.workspaceId == null ||
						Number(task.workspace_id) === Number(props.workspaceId);
					if (sameWorkspace) load();
				};
				timerSubscription = subscribeToUser(userId, {
					onTaskCountdownStarted: reloadIfThisTask,
					onTaskCountdownStopped: reloadIfThisTask,
				});
			};

			watch(() => store.state.user?.id, followTimer, { immediate: true });
			watch(
				() => props.taskId,
				() => {
					cost.value = null;
					costState.value = 'loading';
					localCost.value = null;
					usageReadFor = null;
					runs.value = [];
					loaded.value = false;
					Object.keys(expanded).forEach((id) => delete expanded[id]);
					load();
				},
			);
			onMounted(load);
			onBeforeUnmount(() => {
				clearInterval(ticker);
				subscribe(null);
				followTimer(null);
			});

			const showEmptyState = computed(() =>
				showAgentWorkEmptyState({
					loaded: loaded.value,
					runs: runs.value,
					tourAnchor: active.value?.step.anchor,
				}),
			);

			return {
				runs,
				loaded,
				showEmptyState,
				now,
				expanded,
				STATUS_TONE,
				totals: computed(() =>
					liveTotals(
						runs.value,
						serverTotals.value,
						now.value,
						receivedAt,
						totalsReceivedAt.value,
					),
				),
				receivedAt,
				desktop,
				detailsOpen,
				cost,
				costState,
				workspaceCode,
				blockLine: computed(() => {
					const t = liveTotals(
						runs.value,
						serverTotals.value,
						now.value,
						receivedAt,
						totalsReceivedAt.value,
					);
					const synced =
						cost.value?.ai?.synced && cost.value.ai.costUsd != null
							? Number(cost.value.ai.costUsd)
							: null;
					return (
						blockSummary({
							youSeconds: t.humanSeconds,
							agentSeconds: t.agentSeconds,
							aiCostUsd:
								localCost.value ?? (Number.isFinite(synced) ? synced : null),
							prLabel: prLabel(
								[],
								runs.value.map((r) => r.pr_url),
							),
						}) || 'Details'
					);
				}),
				agentLabel,
				formatWorkDuration,
				isHttpUrl,
				liveSeconds,
				toggle: (id) => {
					expanded[id] = !expanded[id];
				},
				startedLabel: (iso) =>
					formatDistanceToNow(new Date(iso), { addSuffix: true }),
			};
		},
	});
</script>
