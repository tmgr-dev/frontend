<template>
	<div>
		<BaseLayout
			title="AI usage"
			subtitle="Cost is API-equivalent (list prices), not your subscription bill."
		>
			<template #body>
				<div class="flex flex-col gap-6">
					<AutopilotPanel v-if="showAutopilot" />
					<div
						v-if="error"
						role="alert"
						class="flex flex-col gap-3 rounded-lg border border-destructive/40 bg-card p-4 sm:flex-row sm:items-center sm:justify-between"
					>
						<p class="text-sm text-destructive">{{ error }}</p>
						<Button variant="outline" size="sm" @click="load">Retry</Button>
					</div>

					<div
						v-if="scanning"
						role="status"
						class="rounded-lg border border-border bg-card p-3 text-sm text-ink-subtle"
					>
						Reading AI tool logs, the first time can take a few minutes
					</div>

					<div
						v-if="scanIncomplete"
						role="alert"
						class="flex flex-col gap-3 rounded-lg border border-amber-500/40 bg-card p-4 sm:flex-row sm:items-center sm:justify-between"
					>
						<p class="text-sm text-amber-700 dark:text-amber-400">
							Some log files could not be read; numbers may be incomplete
						</p>
						<Button variant="outline" size="sm" @click="load">Retry</Button>
					</div>

					<div
						v-if="!overview && !error"
						class="rounded-lg border border-border bg-card p-6 text-sm text-ink-subtle"
					>
						Loading usage
					</div>

					<SettingsSection
						v-else-if="overview && !overview.supported"
						title="Not available"
					>
						<p class="text-sm text-ink-subtle">
							{{
								overview.unsupported_reason ||
								'AI usage is not available on this computer.'
							}}
						</p>
					</SettingsSection>

					<SettingsSection
						v-else-if="overview && !overview.has_data && !scanning"
						title="No usage yet"
					>
						<p class="text-sm text-ink-subtle">
							TMGR looked for Claude Code and Codex logs on this computer and
							found no usage yet. Use one of them and come back, the numbers
							appear here automatically.
						</p>
					</SettingsSection>

					<template v-else-if="overview">
						<div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
							<div
								v-for="w in overview.windows"
								:key="w.key"
								class="rounded-lg border border-border bg-card p-4"
							>
								<p
									class="text-xs font-medium uppercase tracking-wide text-ink-subtle"
								>
									{{ w.label }}
								</p>
								<p class="text-2xl font-semibold tabular-nums text-ink">
									{{ money(w.totals.cost_usd) }}
								</p>
								<p class="text-xs text-ink-subtle">
									{{ count(w.totals.tokens_total) }} tokens ·
									{{ count(w.totals.events) }} events
								</p>
								<p
									v-if="w.totals.unpriced_events > 0"
									class="mt-1 text-xs text-amber-700 dark:text-amber-400"
								>
									{{ count(w.totals.unpriced_events) }} events have no known
									price and are not counted in the cost.
								</p>
							</div>
						</div>

						<SettingsSection title="Limits">
							<div v-if="overview.limits.length" class="flex flex-col gap-4">
								<div v-for="l in overview.limits" :key="limitKey(l)">
									<div
										class="flex flex-wrap items-baseline justify-between gap-x-3"
									>
										<span class="text-sm font-medium text-ink">
											{{ limitLabel(l) }}
											<span
												v-if="showAccount"
												class="font-normal text-ink-subtle"
											>
												· {{ l.account }}
											</span>
										</span>
										<span class="text-sm tabular-nums text-ink">
											{{ percent(l.used_pct) }}% used
										</span>
									</div>
									<div
										class="mt-1 h-2 overflow-hidden rounded-full bg-muted"
										role="progressbar"
										:aria-valuenow="percent(l.used_pct)"
										aria-valuemin="0"
										aria-valuemax="100"
									>
										<div
											class="h-full rounded-full"
											:class="toneClass(l.used_pct)"
											:style="{ width: percent(l.used_pct) + '%' }"
										/>
									</div>
									<p class="mt-1 text-xs text-ink-subtle">
										Resets in {{ span(l.resets_in_s) }}
										<template v-if="l.plan"> · {{ l.plan }}</template>
										· {{ l.source }} · updated {{ ago(l.age_s) }}
									</p>
								</div>
							</div>
							<p
								v-if="hasAccount('claude') && !hasLimit('claude')"
								class="mt-3 text-sm text-ink-subtle"
							>
								No Claude Code limits yet. Run
								<CommandChip command="tmgr usage statusline install" />
								in a terminal to start collecting them.
							</p>
							<p
								v-if="hasAccount('codex') && !hasLimit('codex')"
								class="mt-3 text-sm text-ink-subtle"
							>
								No Codex limits yet. They appear after your next Codex session.
							</p>
						</SettingsSection>

						<SettingsSection title="By tool and model">
							<template #actions>
								<div
									class="inline-flex rounded-md border border-border p-0.5"
									role="group"
									aria-label="Period"
								>
									<button
										v-for="w in overview.windows"
										:key="w.key"
										type="button"
										class="rounded px-3 py-1 text-sm"
										:class="
											period === w.key
												? 'bg-muted font-medium text-ink'
												: 'text-ink-subtle hover:text-ink'
										"
										:aria-pressed="period === w.key"
										@click="period = w.key"
									>
										{{ w.label }}
									</button>
								</div>
							</template>
							<table v-if="rows.length" class="w-full text-sm max-sm:block">
								<thead
									class="hidden text-left text-xs text-ink-subtle sm:table-header-group"
								>
									<tr>
										<th class="py-2 pr-3 font-medium">Tool</th>
										<th class="py-2 pr-3 font-medium">Model</th>
										<th class="py-2 pr-3 text-right font-medium">Cost</th>
										<th class="py-2 pr-3 text-right font-medium">Tokens</th>
										<th class="py-2 text-right font-medium">Events</th>
									</tr>
								</thead>
								<tbody class="max-sm:block">
									<tr
										v-for="r in rows"
										:key="r.tool + r.model"
										class="block border-t border-border py-2 sm:table-row sm:py-0"
									>
										<td :class="cell" data-label="Tool">
											{{ toolName(r.tool) }}
										</td>
										<td :class="cell" data-label="Model">
											<span class="break-all">{{ r.model }}</span>
										</td>
										<td :class="cellNum" data-label="Cost">
											<span
												>{{ money(r.cost_usd) }}
												<span
													v-if="r.unpriced_events > 0"
													class="text-amber-700 dark:text-amber-400"
													title="Some events have no known price"
													>*</span
												></span
											>
										</td>
										<td :class="cellNum" data-label="Tokens">
											{{ count(r.tokens_total) }}
										</td>
										<td :class="cellNum" data-label="Events">
											{{ count(r.events) }}
										</td>
									</tr>
								</tbody>
							</table>
							<p v-else class="text-sm text-ink-subtle">
								No usage in this period.
							</p>
						</SettingsSection>

						<SettingsSection title="Last 30 days">
							<div v-if="trend.length" class="flex h-32 items-end gap-0.5">
								<div
									v-for="d in trend"
									:key="d.day"
									class="flex h-full min-w-0 flex-1 items-end"
									:title="
										d.day +
										' · ' +
										money(d.cost) +
										' · ' +
										count(d.tokens) +
										' tokens'
									"
								>
									<div
										class="w-full rounded-t bg-ink-subtle"
										:style="{ height: d.height + '%' }"
									/>
								</div>
							</div>
							<p v-else class="text-sm text-ink-subtle">
								No usage in the last 30 days.
							</p>
							<p
								v-if="trend.length"
								class="mt-2 flex justify-between text-xs text-ink-subtle"
							>
								<span>{{ trend[0].day }}</span>
								<span>{{ trend[trend.length - 1].day }}</span>
							</p>
						</SettingsSection>

						<SettingsSection
							title="Tasks"
							description="Attribution is estimated from agent run windows and git branches."
						>
							<p v-if="tasksError" class="text-sm text-ink-subtle">
								{{ tasksError }}
							</p>
							<p v-else-if="tasksPending" class="text-sm text-ink-subtle">
								Matching usage to tasks
							</p>
							<template v-else-if="tasks">
								<table
									v-if="taskRows.length"
									class="w-full text-sm max-sm:block"
								>
									<thead
										class="hidden text-left text-xs text-ink-subtle sm:table-header-group"
									>
										<tr>
											<th class="py-2 pr-3 font-medium">Task</th>
											<th class="py-2 pr-3 text-right font-medium">Cost</th>
											<th class="py-2 text-right font-medium">Tokens</th>
										</tr>
									</thead>
									<tbody class="max-sm:block">
										<tr
											v-for="t in taskRows"
											:key="t.task_ref"
											class="block border-t border-border py-2 sm:table-row sm:py-0"
										>
											<td :class="cell" data-label="Task">
												<span class="min-w-0">
													<router-link
														:to="t.path"
														class="font-medium text-ink underline-offset-2 hover:underline"
													>
														{{ t.key }}
													</router-link>
													<span
														v-if="showWorkspace"
														class="ml-2 text-xs text-ink-subtle/70"
														>{{ t.workspace }}</span
													>
													<span v-if="t.title" class="ml-2 text-ink-subtle">{{
														t.title
													}}</span>
												</span>
											</td>
											<td :class="cellNum" data-label="Cost">
												<span
													>{{ money(t.cost_usd) }}
													<span
														v-if="t.unpriced_events > 0"
														class="text-amber-700 dark:text-amber-400"
														title="Some events have no known price"
														>*</span
													></span
												>
											</td>
											<td :class="cellNum" data-label="Tokens">
												{{ count(t.tokens_total) }}
											</td>
										</tr>
									</tbody>
								</table>
								<p v-else class="text-sm text-ink-subtle">
									No usage could be matched to a task yet.
								</p>
								<p
									v-if="failedWorkspaces > 0"
									class="mt-3 text-xs text-ink-subtle"
								>
									Runs from {{ failedWorkspaces }}
									{{ failedWorkspaces === 1 ? 'workspace' : 'workspaces' }}
									could not be loaded
								</p>
								<p v-if="runsTruncated" class="mt-3 text-xs text-ink-subtle">
									Only the most recent 5000 runs were used
								</p>
								<p
									v-if="pagesTruncated > 0"
									class="mt-3 text-xs text-ink-subtle"
								>
									Only the most recent runs were read
								</p>
								<p class="mt-3 text-xs text-ink-subtle">
									Ambiguous: {{ money(tasks.ambiguous.cost_usd) }} ·
									Unattributed:
									{{ money(tasks.unattributed.cost_usd) }}
								</p>
							</template>
						</SettingsSection>

						<p v-if="overview.history_floor" class="text-xs text-ink-subtle">
							Only the last 90 days are kept.
						</p>
					</template>

					<p class="text-xs text-ink-subtle">
						TMGR reads the usage counters your AI tools already write on this
						computer: model name, token counts, timestamps, session ids, git
						branches and the project folder. It never reads or stores what you
						or the AI wrote. Data stays on this device.
					</p>
				</div>
			</template>
		</BaseLayout>
	</div>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import AutopilotPanel from '@/components/runner/AutopilotPanel.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { isRunnerSupported } from '@/runner/api';
	import { useRunner } from '@/runner/store';
	import store from '@/store';
	import {
		errorText,
		usageOverview,
		usageScan,
		usageTasks,
		type UsageLimit,
		type UsageOverview,
		type UsageTasks,
	} from '@/usage/api';
	import {
		ago,
		barTone,
		count,
		limitLabel,
		money,
		percent,
		span,
		toolName,
	} from '@/usage/format';
	import { fetchRuns, taskInfoFor, type TaskInfo } from '@/usage/runs';
	import { desktopWindowLabel } from '@/utils/desktop';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';

	const POLL_MS = 3000;
	const TONES = {
		ok: 'bg-emerald-500',
		warn: 'bg-amber-500',
		danger: 'bg-red-500',
	};

	export default defineComponent({
		name: 'AiUsage',
		components: { AutopilotPanel, Button, CommandChip, SettingsSection },
		setup() {
			const runner = useRunner();
			const showAutopilot = computed(
				() =>
					isRunnerSupported() &&
					desktopWindowLabel() === 'main' &&
					runner.enabled.value,
			);
			const overview = ref<UsageOverview | null>(null),
				error = ref<string | null>(null),
				scanning = ref(false),
				period = ref<'today' | 'week' | 'month'>('week'),
				tasks = ref<UsageTasks | null>(null),
				taskInfos = ref<Record<string, TaskInfo>>({}),
				tasksPending = ref(true),
				scanIncomplete = ref(false),
				failedWorkspaces = ref(0),
				pagesTruncated = ref(0),
				runsTruncated = ref(false),
				tasksError = ref<string | null>(null);
			let disposed = false;
			let timer: ReturnType<typeof setTimeout> | null = null;
			let generation = 0;

			const sleep = (ms: number) =>
				new Promise<void>((resolve) => {
					timer = setTimeout(resolve, ms);
				});

			async function loadTasks(run: number) {
				tasksPending.value = true;
				tasksError.value = null;
				try {
					const fetched = await fetchRuns(store.state.workspaces ?? []);
					const result = await usageTasks(fetched.runs);
					if (disposed || run !== generation) return;
					taskInfos.value = fetched.tasks;
					failedWorkspaces.value = fetched.failedWorkspaces;
					pagesTruncated.value = fetched.truncated;
					runsTruncated.value = result.runs_truncated;
					tasks.value = result;
				} catch (e) {
					if (disposed || run !== generation) return;
					tasksError.value = `Could not match usage to tasks: ${errorText(e)}`;
				} finally {
					if (!disposed && run === generation) tasksPending.value = false;
				}
			}

			async function load() {
				const run = ++generation;
				error.value = null;
				scanIncomplete.value = false;
				try {
					overview.value = await usageOverview();
					if (disposed || run !== generation) return;
					if (!overview.value.supported) return;
					scanning.value = true;
					let scan = await usageScan();
					while (scan.state === 'busy' && !disposed && run === generation) {
						await sleep(POLL_MS);
						if (disposed || run !== generation) return;
						scan = await usageScan();
					}
					if (disposed || run !== generation) return;
					scanning.value = false;
					scanIncomplete.value = scan.errors > 0 || scan.bad_lines > 0;
					overview.value = await usageOverview();
				} catch (e) {
					if (!disposed && run === generation) error.value = errorText(e);
					return;
				} finally {
					if (!disposed && run === generation) scanning.value = false;
				}
				if (!disposed && run === generation) await loadTasks(run);
			}

			onMounted(() => {
				setDocumentTitle('AI usage');
				void load();
			});
			onBeforeUnmount(() => {
				disposed = true;
				if (timer) clearTimeout(timer);
			});

			const rows = computed(
				() =>
					overview.value?.windows.find((w) => w.key === period.value)?.rows ??
					[],
			);
			const trend = computed(() => {
				const byDay = new Map<string, { cost: number; tokens: number }>();
				for (const d of overview.value?.days ?? []) {
					const entry = byDay.get(d.day) ?? { cost: 0, tokens: 0 };
					entry.cost += d.cost_usd;
					entry.tokens += d.tokens_total;
					byDay.set(d.day, entry);
				}
				const days = [...byDay.entries()]
					.sort(([a], [b]) => a.localeCompare(b))
					.map(([day, v]) => ({ day, ...v }));
				const max = Math.max(...days.map((d) => d.cost), 0);
				return days.map((d) => ({
					...d,
					height: max > 0 ? Math.max(2, Math.round((d.cost / max) * 100)) : 2,
				}));
			});
			const taskRows = computed(() =>
				(tasks.value?.tasks ?? []).map((t) => ({
					...t,
					...taskInfoFor(taskInfos.value, t.task_ref),
				})),
			);
			const showWorkspace = computed(
				() => new Set(taskRows.value.map((t) => t.workspace)).size > 1,
			);
			const showAccount = computed(
				() => new Set(overview.value?.limits.map((l) => l.account)).size > 1,
			);
			const hasAccount = (tool: string) =>
				!!overview.value?.accounts.some((a) => a.tool === tool);
			const hasLimit = (tool: string) =>
				!!overview.value?.limits.some((l) => l.tool === tool);

			return {
				showAutopilot,
				overview,
				error,
				scanning,
				period,
				tasks,
				tasksPending,
				tasksError,
				rows,
				trend,
				taskRows,
				showAccount,
				showWorkspace,
				hasLimit,
				hasAccount,
				scanIncomplete,
				failedWorkspaces,
				pagesTruncated,
				runsTruncated,
				load,
				money,
				count,
				span,
				ago,
				percent,
				toolName,
				limitLabel,
				limitKey: (l: UsageLimit) => `${l.tool}/${l.account}/${l.window}`,
				toneClass: (value: number) => TONES[barTone(value)],
				cell: 'py-1 sm:table-cell sm:py-2 sm:pr-3 max-sm:flex max-sm:justify-between max-sm:gap-4 max-sm:text-right max-sm:before:text-left max-sm:before:text-ink-subtle max-sm:before:content-[attr(data-label)]',
				cellNum:
					'py-1 tabular-nums sm:table-cell sm:py-2 sm:pr-3 sm:text-right sm:last:pr-0 max-sm:flex max-sm:justify-between max-sm:gap-4 max-sm:text-right max-sm:before:text-left max-sm:before:text-ink-subtle max-sm:before:content-[attr(data-label)]',
			};
		},
	});
</script>
