<template>
	<div>
		<BaseLayout
			:title="$t('agents.aiUsage.title')"
			:subtitle="$t('agents.aiUsage.subtitle')"
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
						<Button variant="outline" size="sm" @click="load">{{
							$t('common.actions.retry')
						}}</Button>
					</div>

					<div
						v-if="scanning"
						role="status"
						class="rounded-lg border border-border bg-card p-3 text-sm text-ink-subtle"
					>
						{{ $t('agents.aiUsage.scanning') }}
					</div>

					<div
						v-if="scanIncomplete"
						role="alert"
						class="flex flex-col gap-3 rounded-lg border border-amber-500/40 bg-card p-4 sm:flex-row sm:items-center sm:justify-between"
					>
						<p class="text-sm text-amber-700 dark:text-amber-400">
							{{ $t('agents.aiUsage.scanIncomplete') }}
						</p>
						<Button variant="outline" size="sm" @click="load">{{
							$t('common.actions.retry')
						}}</Button>
					</div>

					<UsageConsentGate
						v-if="consent && gated"
						:consent="consent"
						@update="onConsent"
					/>

					<div
						v-if="!overview && !error && !gated"
						class="rounded-lg border border-border bg-card p-6 text-sm text-ink-subtle"
					>
						{{ $t('agents.aiUsage.loading') }}
					</div>

					<SettingsSection
						v-else-if="overview && !overview.supported"
						:title="$t('agents.aiUsage.notAvailable')"
					>
						<p class="text-sm text-ink-subtle">
							{{
								overview.unsupported_reason || $t('agents.aiUsage.unsupported')
							}}
						</p>
					</SettingsSection>

					<SettingsSection
						v-else-if="overview && !overview.has_data && !scanning"
						:title="$t('agents.aiUsage.noUsageYet')"
					>
						<p class="text-sm text-ink-subtle">
							{{ $t('agents.aiUsage.noUsageBody') }}
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
									class="text-xs font-medium tracking-wide text-ink-subtle uppercase"
								>
									{{ w.label }}
								</p>
								<p class="text-2xl font-semibold text-ink tabular-nums">
									{{ money(w.totals.cost_usd) }}
								</p>
								<p class="text-xs text-ink-subtle">
									{{
										$t('agents.aiUsage.tokens', {
											n: count(w.totals.tokens_total),
										})
									}}
									·
									{{
										$t('agents.aiUsage.events', { n: count(w.totals.events) })
									}}
								</p>
								<p
									v-if="w.totals.unpriced_events > 0"
									class="mt-1 text-xs text-amber-700 dark:text-amber-400"
								>
									{{
										$t('agents.aiUsage.unpriced', {
											n: count(w.totals.unpriced_events),
										})
									}}
								</p>
							</div>
						</div>

						<UsageWeekStrip :days="overview.days" />

						<SettingsSection :title="$t('agents.aiUsage.limits')">
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
										<span class="text-sm text-ink tabular-nums">
											{{
												$t('agents.aiUsage.usedPct', {
													pct: percent(l.used_pct),
												})
											}}
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
										{{
											$t('agents.aiUsage.resetsIn', {
												span: span(l.resets_in_s),
											})
										}}
										<template v-if="l.plan"> · {{ l.plan }}</template>
										· {{ l.source }} ·
										{{ $t('agents.aiUsage.updated', { ago: ago(l.age_s) }) }}
									</p>
									<LimitForecast :limit="l" />
								</div>
							</div>
							<i18n-t
								v-if="hasAccount('claude') && !hasLimit('claude')"
								keypath="agents.aiUsage.noClaudeLimits"
								tag="p"
								class="mt-3 text-sm text-ink-subtle"
							>
								<template #command>
									<CommandChip command="tmgr usage statusline install" />
								</template>
							</i18n-t>
							<p
								v-if="hasAccount('codex') && !hasLimit('codex')"
								class="mt-3 text-sm text-ink-subtle"
							>
								{{ $t('agents.aiUsage.noCodexLimits') }}
							</p>
						</SettingsSection>

						<UsageDailyChart :days="overview.days" />

						<SettingsSection :title="$t('agents.aiUsage.byToolModel')">
							<template #actions>
								<div
									class="inline-flex rounded-md border border-border p-0.5"
									role="group"
									:aria-label="$t('agents.aiUsage.period')"
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
										<th class="py-2 pr-3 font-medium">
											{{ $t('agents.aiUsage.tool') }}
										</th>
										<th class="py-2 pr-3 font-medium">
											{{ $t('agents.aiUsage.model') }}
										</th>
										<th class="py-2 pr-3 text-right font-medium">
											{{ $t('agents.aiUsage.cost') }}
										</th>
										<th class="py-2 pr-3 text-right font-medium">
											{{ $t('agents.aiUsage.tokensHeader') }}
										</th>
										<th class="py-2 text-right font-medium">
											{{ $t('agents.aiUsage.eventsHeader') }}
										</th>
									</tr>
								</thead>
								<tbody class="max-sm:block">
									<tr
										v-for="r in rows"
										:key="r.tool + r.model"
										class="block border-t border-border py-2 sm:table-row sm:py-0"
									>
										<td :class="cell" :data-label="$t('agents.aiUsage.tool')">
											{{ toolName(r.tool) }}
										</td>
										<td :class="cell" :data-label="$t('agents.aiUsage.model')">
											<span class="break-all">{{ r.model }}</span>
										</td>
										<td
											:class="cellNum"
											:data-label="$t('agents.aiUsage.cost')"
										>
											<span
												>{{ money(r.cost_usd) }}
												<span
													v-if="r.unpriced_events > 0"
													class="text-amber-700 dark:text-amber-400"
													:title="$t('agents.aiUsage.unpricedTitle')"
													>*</span
												></span
											>
										</td>
										<td
											:class="cellNum"
											:data-label="$t('agents.aiUsage.tokensHeader')"
										>
											{{ count(r.tokens_total) }}
										</td>
										<td
											:class="cellNum"
											:data-label="$t('agents.aiUsage.eventsHeader')"
										>
											{{ count(r.events) }}
										</td>
									</tr>
								</tbody>
							</table>
							<p v-else class="text-sm text-ink-subtle">
								{{ $t('agents.usage.noUsage') }}
							</p>
						</SettingsSection>

						<UsageTokenTypes
							v-if="currentWindow"
							:totals="currentWindow.totals"
							:label="currentWindow.label"
						/>

						<UsageProjects
							v-if="currentWindow"
							:projects="currentWindow.projects"
							:label="currentWindow.label"
						/>

						<SettingsSection
							:title="$t('agents.aiUsage.tasks')"
							:description="$t('agents.aiUsage.tasksDescription')"
						>
							<p v-if="tasksError" class="text-sm text-ink-subtle">
								{{ tasksError }}
							</p>
							<p v-else-if="tasksPending" class="text-sm text-ink-subtle">
								{{ $t('agents.aiUsage.matching') }}
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
											<th class="py-2 pr-3 font-medium">
												{{ $t('agents.aiUsage.task') }}
											</th>
											<th class="py-2 pr-3 text-right font-medium">
												{{ $t('agents.aiUsage.cost') }}
											</th>
											<th class="py-2 text-right font-medium">
												{{ $t('agents.aiUsage.tokensHeader') }}
											</th>
										</tr>
									</thead>
									<tbody class="max-sm:block">
										<tr
											v-for="t in taskRows"
											:key="t.task_ref"
											class="block border-t border-border py-2 sm:table-row sm:py-0"
										>
											<td :class="cell" :data-label="$t('agents.aiUsage.task')">
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
											<td
												:class="cellNum"
												:data-label="$t('agents.aiUsage.cost')"
											>
												<span
													>{{ money(t.cost_usd) }}
													<span
														v-if="t.unpriced_events > 0"
														class="text-amber-700 dark:text-amber-400"
														:title="$t('agents.aiUsage.unpricedTitle')"
														>*</span
													></span
												>
											</td>
											<td
												:class="cellNum"
												:data-label="$t('agents.aiUsage.tokensHeader')"
											>
												{{ count(t.tokens_total) }}
											</td>
										</tr>
									</tbody>
								</table>
								<p v-else class="text-sm text-ink-subtle">
									{{ $t('agents.aiUsage.noMatch') }}
								</p>
								<p
									v-if="failedWorkspaces > 0"
									class="mt-3 text-xs text-ink-subtle"
								>
									{{ $t('agents.aiUsage.failedWorkspaces', failedWorkspaces) }}
								</p>
								<p v-if="runsTruncated" class="mt-3 text-xs text-ink-subtle">
									{{ $t('agents.aiUsage.runsTruncated') }}
								</p>
								<p
									v-if="pagesTruncated > 0"
									class="mt-3 text-xs text-ink-subtle"
								>
									{{ $t('agents.aiUsage.pagesTruncated') }}
								</p>
								<p class="mt-3 text-xs text-ink-subtle">
									{{
										$t('agents.aiUsage.ambiguous', {
											cost: money(tasks.ambiguous.cost_usd),
										})
									}}
									·
									{{
										$t('agents.aiUsage.unattributed', {
											cost: money(tasks.unattributed.cost_usd),
										})
									}}
								</p>
							</template>
						</SettingsSection>

						<p v-if="overview.history_floor" class="text-xs text-ink-subtle">
							{{ $t('agents.aiUsage.historyFloor') }}
						</p>
					</template>

					<p
						v-if="consent?.state === 'granted'"
						class="text-xs text-ink-subtle"
					>
						{{ $t('agents.aiUsage.privacyNote') }}
					</p>
				</div>
			</template>
		</BaseLayout>
	</div>
</template>

<script lang="ts">
	import CommandChip from '@/components/general/CommandChip.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import LimitForecast from '@/components/usage/LimitForecast.vue';
	import UsageDailyChart from '@/components/usage/UsageDailyChart.vue';
	import UsageProjects from '@/components/usage/UsageProjects.vue';
	import UsageTokenTypes from '@/components/usage/UsageTokenTypes.vue';
	import UsageWeekStrip from '@/components/usage/UsageWeekStrip.vue';
	import AutopilotPanel from '@/components/runner/AutopilotPanel.vue';
	import { consentGate } from '@/components/usage/consent';
	import UsageConsentGate from '@/components/usage/UsageConsentGate.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';
	import { isRunnerSupported } from '@/runner/api';
	import { useRunner } from '@/runner/store';
	import store from '@/store';
	import {
		errorText,
		usageConsent,
		usageOverview,
		usageScan,
		usageTasks,
		type UsageConsent,
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
		components: {
			AutopilotPanel,
			Button,
			CommandChip,
			LimitForecast,
			SettingsSection,
			UsageConsentGate,
			UsageDailyChart,
			UsageProjects,
			UsageTokenTypes,
			UsageWeekStrip,
		},
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
				tasksError = ref<string | null>(null),
				consent = ref<UsageConsent | null>(null);
			const gated = computed(() =>
				['ask', 'off'].includes(consentGate(consent.value) ?? ''),
			);
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
					tasksError.value = t('agents.aiUsage.matchFailed', {
						error: errorText(e),
					});
				} finally {
					if (!disposed && run === generation) tasksPending.value = false;
				}
			}

			async function load() {
				const run = ++generation;
				error.value = null;
				scanIncomplete.value = false;
				try {
					consent.value = await usageConsent();
					if (disposed || run !== generation) return;
					if (gated.value) {
						overview.value = null;
						return;
					}
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
					if (scan.state === 'off') {
						consent.value = await usageConsent();
						if (disposed || run !== generation) return;
						if (gated.value) {
							overview.value = null;
							return;
						}
					}
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

			const onConsent = (next: UsageConsent) => {
				consent.value = next;
				if (!gated.value) void load();
			};

			onMounted(() => {
				setDocumentTitle(t('agents.aiUsage.documentTitle'));
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
			const currentWindow = computed(() =>
				overview.value?.windows.find((w) => w.key === period.value),
			);
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
				consent,
				gated,
				onConsent,
				overview,
				error,
				scanning,
				period,
				tasks,
				tasksPending,
				tasksError,
				rows,
				currentWindow,
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
