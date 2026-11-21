<template>
	<div>
		<BaseLayout
			:title="$t('agents.usage.machines.title')"
			:subtitle="$t('agents.usage.machines.subtitle')"
		>
			<template v-if="isDesktop" #header-actions>
				<AiUsageTabs current="all" />
			</template>
			<template #body>
				<div class="flex flex-col gap-6">
					<div
						v-if="error"
						role="alert"
						class="flex flex-col gap-3 rounded-lg border border-destructive/40 bg-card p-4 sm:flex-row sm:items-center sm:justify-between"
					>
						<p class="text-sm text-destructive">{{ error }}</p>
						<Button variant="outline" size="sm" @click="reload">{{
							$t('common.actions.retry')
						}}</Button>
					</div>

					<div
						v-else-if="!machinesLoaded"
						role="status"
						class="rounded-lg border border-border bg-card p-6 text-sm text-ink-subtle"
					>
						{{ $t('agents.usage.machines.loading') }}
					</div>

					<SettingsSection
						v-else-if="!machines.length"
						:title="$t('agents.usage.machines.empty.title')"
					>
						<p class="text-sm text-ink-subtle" data-testid="machines-empty">
							<i18n-t keypath="agents.usage.machines.empty.body">
								<template #command>
									<CommandChip command="tmgr usage machine sync" />
								</template>
							</i18n-t>
						</p>
					</SettingsSection>

					<template v-else>
						<p v-if="removeError" role="alert" class="text-sm text-destructive">
							{{ removeError }}
						</p>

						<div
							class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3"
							data-testid="machine-cards"
						>
							<MachineLimitCard
								v-for="m in machines"
								:key="m.machine_id"
								:machine="m"
								:now="now"
								@remove="remove"
							/>
						</div>

						<div class="flex flex-wrap items-end gap-3" data-testid="filters">
							<div
								class="inline-flex rounded-md border border-border p-0.5"
								role="group"
								:aria-label="$t('agents.usage.machines.period.label')"
							>
								<button
									v-for="p in periods"
									:key="p.key"
									type="button"
									class="rounded px-3 py-1 text-sm"
									:class="
										period === p.key
											? 'bg-muted font-medium text-ink'
											: 'text-ink-subtle hover:text-ink'
									"
									:aria-pressed="period === p.key"
									:data-period="p.key"
									@click="period = p.key"
								>
									{{ p.label }}
								</button>
							</div>
							<label
								v-for="f in selects"
								:key="f.key"
								class="flex min-w-0 flex-col gap-1 text-xs text-ink-subtle"
							>
								{{ f.label }}
								<select
									v-model="filters[f.key]"
									class="h-9 max-w-full rounded-md border border-input bg-background px-2 text-sm text-ink"
									:data-filter="f.key"
								>
									<option value="">
										{{ $t('agents.usage.machines.filters.all') }}
									</option>
									<option v-for="o in f.options" :key="o.id" :value="o.id">
										{{ o.label }}
									</option>
								</select>
							</label>
						</div>

						<p
							v-if="dailyError"
							role="alert"
							class="flex flex-wrap items-center gap-3 text-sm text-destructive"
						>
							{{ dailyError }}
							<Button variant="outline" size="sm" @click="loadDaily">{{
								$t('common.actions.retry')
							}}</Button>
						</p>
						<p
							v-else-if="!dailyLoaded"
							role="status"
							class="text-sm text-ink-subtle"
						>
							{{ $t('agents.usage.machines.loading') }}
						</p>

						<template v-if="dailyLoaded">
							<p
								v-if="truncated"
								class="text-sm text-amber-700 dark:text-amber-400"
							>
								{{ $t('agents.usage.machines.truncated') }}
							</p>

							<div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
								<div
									v-for="tile in tiles"
									:key="tile.key"
									class="rounded-lg border border-border bg-card p-4"
									:title="tile.hint"
								>
									<p
										class="text-xs font-medium tracking-wide text-ink-subtle uppercase"
									>
										{{ tile.label }}
									</p>
									<p
										class="text-2xl font-semibold text-ink tabular-nums"
										:data-testid="`tile-${tile.key}`"
									>
										{{ tile.value }}
									</p>
								</div>
							</div>

							<MachineTotalsChart
								:rows="rows"
								:machines="machines"
								:range="range"
							/>

							<SettingsSection
								v-if="groups.length"
								:title="$t('agents.usage.machines.table.title')"
							>
								<table class="w-full text-sm" data-testid="machines-table">
									<thead :class="headClass">
										<tr>
											<th class="py-2 pr-3 font-medium">
												{{ $t('agents.usage.machines.table.model') }}
											</th>
											<th class="py-2 pr-3 font-medium">
												{{ $t('agents.usage.machines.table.task') }}
											</th>
											<th class="py-2 pr-3 text-right font-medium">
												{{ $t('agents.usage.machines.totals.cost') }}
											</th>
											<th class="py-2 pr-3 text-right font-medium">
												{{ $t('agents.usage.machines.totals.tokens') }}
											</th>
											<th
												class="py-2 text-right font-medium"
												:title="$t('agents.usage.machines.totals.sessionsHint')"
											>
												{{ $t('agents.usage.machines.totals.sessions') }}
											</th>
										</tr>
									</thead>
									<tbody class="max-sm:block">
										<tr v-for="g in groups" :key="g.key" :class="rowClass">
											<td
												:class="cell"
												:data-label="$t('agents.usage.machines.table.model')"
											>
												<span class="break-all"
													>{{ g.model }}
													<span class="text-ink-subtle"
														>· {{ agentName(g.agent) }}</span
													></span
												>
											</td>
											<td
												:class="cell"
												:data-label="$t('agents.usage.machines.table.task')"
											>
												{{
													g.task ??
													$t('agents.usage.machines.filters.unassigned')
												}}
											</td>
											<td
												:class="cellNum"
												:data-label="$t('agents.usage.machines.totals.cost')"
											>
												{{ money(g.cost) }}
											</td>
											<td
												:class="cellNum"
												:data-label="$t('agents.usage.machines.totals.tokens')"
											>
												{{ count(g.tokens) }}
											</td>
											<td
												:class="cellNum"
												:data-label="
													$t('agents.usage.machines.totals.sessions')
												"
											>
												{{ count(g.sessions) }}
											</td>
										</tr>
									</tbody>
								</table>
							</SettingsSection>
						</template>
					</template>
				</div>
			</template>
		</BaseLayout>
	</div>
</template>

<script lang="ts">
	import {
		fetchDaily,
		fetchMachines,
		removeMachine,
		type DailyRow,
		type Machine,
	} from '@/actions/tmgr/aiUsageMachines';
	import CommandChip from '@/components/general/CommandChip.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import AiUsageTabs from '@/components/usage/machines/AiUsageTabs.vue';
	import MachineLimitCard from '@/components/usage/machines/MachineLimitCard.vue';
	import MachineTotalsChart from '@/components/usage/machines/MachineTotalsChart.vue';
	import {
		cell,
		cellNum,
		headClass,
		rowClass,
	} from '@/components/usage/tableClasses';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';
	import { count, money } from '@/usage/format';
	import {
		agentName,
		aggregateTotals,
		dailyQuery,
		emptyFilters,
		filterOptions,
		groupByModelTask,
		isUnassigned,
		PERIODS,
		periodRange,
		UNASSIGNED,
		type Filters,
		type Period,
	} from '@/usage/machines';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';

	const TICK_MS = 30_000;

	const errorText = (e: unknown): string => {
		const err = e as any;
		return err?.response?.data?.message || err?.message || String(e);
	};

	export default defineComponent({
		name: 'AiUsageMachines',
		components: {
			AiUsageTabs,
			Button,
			CommandChip,
			MachineLimitCard,
			MachineTotalsChart,
			SettingsSection,
		},
		setup() {
			const machines = ref<Machine[]>([]);
			const machinesLoaded = ref(false);
			const allRows = ref<DailyRow[]>([]);
			const optionRows = ref<DailyRow[]>([]);
			const dailyLoaded = ref(false);
			const truncated = ref(false);
			const error = ref('');
			const dailyError = ref('');
			const removeError = ref('');
			const period = ref<Period>('30d');
			const filters = reactive<Filters>(emptyFilters());
			const now = ref(new Date());
			let timer: ReturnType<typeof setInterval> | null = null;
			let generation = 0;
			let disposed = false;

			const range = computed(() => periodRange(period.value, now.value));
			const noFilters = () =>
				!filters.machineId &&
				!filters.agent &&
				!filters.model &&
				!filters.taskId;

			const loadDaily = async () => {
				const run = ++generation;
				const query = dailyQuery(range.value, filters);
				dailyError.value = '';
				try {
					const res = await fetchDaily(query);
					if (disposed || run !== generation) return;
					allRows.value = res.rows;
					truncated.value = res.truncated;
					if (noFilters()) optionRows.value = res.rows;
					dailyLoaded.value = true;
				} catch (e) {
					if (disposed || run !== generation) return;
					dailyError.value = t('agents.usage.machines.error', {
						error: errorText(e),
					});
				}
			};

			const loadMachines = async () => {
				error.value = '';
				try {
					const list = await fetchMachines();
					if (disposed) return;
					machines.value = list;
					machinesLoaded.value = true;
				} catch (e) {
					if (disposed) return;
					error.value = t('agents.usage.machines.error', {
						error: errorText(e),
					});
				}
			};

			const reload = async () => {
				await loadMachines();
				if (!error.value) await loadDaily();
			};

			const rows = computed(() =>
				filters.taskId === UNASSIGNED
					? allRows.value.filter(isUnassigned)
					: allRows.value,
			);
			const options = computed(() =>
				filterOptions([...optionRows.value, ...allRows.value], machines.value),
			);
			const totals = computed(() => aggregateTotals(rows.value));

			watch(
				[() => range.value.from, () => range.value.to, () => ({ ...filters })],
				() => void loadDaily(),
			);

			onMounted(() => {
				setDocumentTitle(t('agents.usage.machines.title'));
				timer = setInterval(() => {
					now.value = new Date();
				}, TICK_MS);
				void reload();
			});
			onBeforeUnmount(() => {
				disposed = true;
				if (timer) clearInterval(timer);
			});

			return {
				isDesktop: isDesktopApp(),
				machines,
				machinesLoaded,
				rows,
				dailyLoaded,
				truncated,
				error,
				dailyError,
				removeError,
				period,
				filters,
				now,
				range,
				groups: computed(() => groupByModelTask(rows.value)),
				periods: computed(() =>
					PERIODS.map((p) => ({
						key: p,
						label: t(
							`agents.usage.machines.period.${p === 'today' ? 'today' : `d${p.slice(0, -1)}`}`,
						),
					})),
				),
				selects: computed(() => [
					{
						key: 'machineId' as const,
						label: t('agents.usage.machines.filters.machine'),
						options: options.value.machines,
					},
					{
						key: 'agent' as const,
						label: t('agents.usage.machines.filters.agent'),
						options: options.value.agents.map((a) => ({
							id: a,
							label: agentName(a),
						})),
					},
					{
						key: 'model' as const,
						label: t('agents.usage.machines.filters.model'),
						options: options.value.models.map((m) => ({ id: m, label: m })),
					},
					{
						key: 'taskId' as const,
						label: t('agents.usage.machines.filters.task'),
						options: [
							...(options.value.hasUnassigned
								? [
										{
											id: UNASSIGNED,
											label: t('agents.usage.machines.filters.unassigned'),
										},
									]
								: []),
							...options.value.tasks,
						],
					},
				]),
				tiles: computed(() => [
					{
						key: 'cost',
						label: t('agents.usage.machines.totals.cost'),
						value: money(totals.value.cost),
						hint: '',
					},
					{
						key: 'tokens',
						label: t('agents.usage.machines.totals.tokens'),
						value: count(totals.value.tokens),
						hint: '',
					},
					{
						key: 'sessions',
						label: t('agents.usage.machines.totals.sessions'),
						value: count(totals.value.sessions),
						hint: t('agents.usage.machines.totals.sessionsHint'),
					},
				]),
				reload,
				loadDaily,
				remove: async (machineId: string) => {
					removeError.value = '';
					try {
						await removeMachine(machineId);
						await reload();
					} catch (e) {
						removeError.value = t('agents.usage.machines.remove.failed', {
							error: errorText(e),
						});
					}
				},
				agentName,
				count,
				money,
				cell,
				cellNum,
				headClass,
				rowClass,
			};
		},
	});
</script>
