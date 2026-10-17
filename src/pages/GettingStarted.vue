<template>
	<PageContainer width="wide" class="getting-started">
		<PageHeader
			title="Getting started"
			subtitle="Short guided tours. Do them in any order, skip any of them."
		>
			<template #actions>
				<div class="flex items-center gap-3" data-tour="hub.tips-toggle">
					<label
						id="hub-tips-label"
						class="text-sm font-medium text-ink"
						for="hub-tips-switch"
					>
						Show tips automatically
					</label>
					<Switch
						id="hub-tips-switch"
						aria-labelledby="hub-tips-label"
						:checked="tipsOn"
						@update:checked="toggleTips"
					/>
					<span class="h-5 w-px bg-line" aria-hidden="true" />
					<button
						type="button"
						class="text-sm text-ink-subtle underline hover:text-ink"
						data-testid="hub-hide"
						:disabled="busy"
						@click="toggleHubHidden"
					>
						{{ hubHidden ? 'Show in sidebar' : 'Hide this page' }}
					</button>
				</div>
			</template>
		</PageHeader>

		<div data-tour="hub.checklist" class="mb-6 flex flex-col gap-3">
			<ChecklistCard />
			<RemoveSampleTasksButton class="self-start" />
			<section
				class="rounded-card border border-line bg-surface p-4 dark:border-gray-700 dark:bg-gray-900"
				aria-labelledby="hub-progress-label"
			>
				<p id="hub-progress-label" class="text-sm font-medium text-ink">
					{{ model.done }} of {{ model.total }} tours done
				</p>
				<div
					class="mt-2 h-2 w-full max-w-md overflow-hidden rounded-full bg-surface-sunken dark:bg-gray-800"
					role="progressbar"
					aria-labelledby="hub-progress-label"
					aria-valuemin="0"
					:aria-valuemax="model.total"
					:aria-valuenow="model.done"
				>
					<div
						class="h-full rounded-full bg-brand transition-all"
						:style="{ width: progressWidth }"
					/>
				</div>
			</section>
		</div>

		<div data-tour="hub.catalog" class="grid grid-cols-1 gap-4 md:grid-cols-2">
			<section
				v-for="group in model.groups"
				:key="group.group"
				class="rounded-card border border-line bg-surface p-4 dark:border-gray-700 dark:bg-gray-900"
				:aria-labelledby="`hub-group-${group.group}`"
			>
				<h2
					:id="`hub-group-${group.group}`"
					class="mb-3 border-b border-line pb-2 text-base font-semibold text-ink dark:border-gray-700"
				>
					{{ group.title }}
				</h2>
				<ul class="flex flex-col gap-2.5">
					<li
						v-for="row in group.rows"
						:key="row.id"
						class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm"
						:data-tour-id="row.id"
					>
						<span
							:class="
								row.action === 'none' ? 'font-semibold text-ink' : 'text-ink'
							"
						>
							{{ row.title }}
						</span>
						<template v-if="row.disabledMessage">
							<span class="text-ink-subtle">{{ row.disabledMessage }}</span>
						</template>
						<template v-else>
							<span class="text-ink-subtle">{{ row.minutes }} min</span>
							<span
								v-if="row.updated"
								class="rounded bg-amber-100 px-1.5 py-0.5 text-2xs font-semibold text-amber-800 dark:bg-amber-900/40 dark:text-amber-200"
							>
								Updated
							</span>
							<span
								v-if="row.action === 'done'"
								class="rounded bg-surface-sunken px-1.5 py-0.5 text-2xs font-semibold text-ink-muted dark:bg-gray-800 dark:text-gray-300"
							>
								Done
							</span>
							<button
								v-else
								type="button"
								class="min-h-[28px] rounded bg-brand px-2.5 py-0.5 text-xs font-semibold text-white hover:opacity-90 disabled:opacity-50"
								:data-testid="`hub-${row.action}-${row.id}`"
								:aria-label="`${row.action === 'resume' ? 'Resume' : 'Start'} ${
									row.title
								}`"
								:disabled="busy"
								@click="run(row)"
							>
								{{ row.action === 'resume' ? 'Resume' : 'Start' }}
							</button>
							<span v-if="row.progress" class="text-2xs text-ink-subtle">
								({{ row.progress }})
							</span>
						</template>
					</li>
				</ul>
			</section>
		</div>
	</PageContainer>
</template>

<script lang="ts">
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import ChecklistCard from '@/components/onboarding/ChecklistCard.vue';
	import RemoveSampleTasksButton from '@/components/onboarding/RemoveSampleTasksButton.vue';
	import { Switch } from '@/components/ui/switch';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { buildHubModel, type HubRow } from '@/tours/hubModel';
	import { useTours } from '@/tours/useTours';
	import { computed, defineComponent, ref } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'GettingStarted',
		components: {
			ChecklistCard,
			PageContainer,
			PageHeader,
			RemoveSampleTasksButton,
			Switch,
		},
		setup() {
			const store = useStore();
			const { isFeatureEnabled } = useFeatureToggles();
			const tours = useTours();
			const busy = ref(false);

			const model = computed(() =>
				buildHubModel(tours.hubItems.value, {
					canManageModules:
						store.getters['featureToggles/canManageModules'] === true,
					isModuleEnabled: (key: string) => isFeatureEnabled(key),
				}),
			);
			const progressWidth = computed(() =>
				model.value.total
					? `${Math.round((model.value.done / model.value.total) * 100)}%`
					: '0%',
			);
			void tours.ensurePersonas();
			const tipsOn = computed(
				() => tours.onboarding.value?.tips_mode === 'auto',
			);
			const hubHidden = computed(
				() => tours.onboarding.value?.hub_hidden === true,
			);

			const guarded = async (work: () => Promise<void>) => {
				busy.value = true;
				try {
					await work();
				} catch (error) {
					console.error(error);
				} finally {
					busy.value = false;
				}
			};

			return {
				model,
				progressWidth,
				tipsOn,
				hubHidden,
				busy,
				toggleTips: (on: boolean) =>
					guarded(() => tours.setTipsMode(on ? 'auto' : 'off')),
				toggleHubHidden: () =>
					guarded(() => tours.setHubHidden(!hubHidden.value)),
				run: (row: HubRow) =>
					guarded(() =>
						row.action === 'resume'
							? tours.resume(row.id)
							: tours.start(row.id, 'hub'),
					),
			};
		},
	});
</script>
