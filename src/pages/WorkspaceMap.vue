<template>
	<BaseLayout no-copyright width="full">
		<template #body>
			<div
				v-if="enabled"
				class="flex min-h-0 flex-1 flex-col gap-4 px-4 pb-4 pt-4 md:px-6 md:pt-6"
				data-testid="workspace-map"
			>
				<header
					class="flex flex-wrap items-end justify-between gap-x-4 gap-y-3"
				>
					<div class="flex min-w-0 flex-col gap-1">
						<div class="text-sm text-ink-muted">
							Workspace<template v-if="workspaceCode">
								· {{ workspaceCode }}</template
							>
						</div>
						<h1 class="text-xl font-semibold sm:text-2xl">
							Map — how everything connects
						</h1>
					</div>
					<div class="flex flex-wrap items-center gap-2">
						<div class="relative">
							<label
								class="flex items-center gap-2 rounded-xl border border-line bg-surface px-3 py-2 text-sm text-ink-muted"
							>
								<Search class="h-4 w-4 shrink-0" />
								<input
									v-model="query"
									type="text"
									placeholder="task, page, person"
									aria-label="Search the map"
									role="combobox"
									aria-autocomplete="list"
									aria-controls="map-results"
									:aria-expanded="resultsVisible"
									:aria-activedescendant="
										active >= 0 ? `map-result-${active}` : undefined
									"
									class="w-36 bg-transparent text-ink outline-none placeholder:text-ink-subtle"
									data-testid="map-search"
									@focus="listOpen = true"
									@blur="listOpen = false"
									@input="onSearchInput"
									@keydown="onSearchKey"
								/>
								<span
									v-if="matches"
									class="shrink-0 text-xs tabular-nums"
									data-testid="map-match-count"
									>{{ matches.length }}</span
								>
							</label>
							<ul
								v-if="resultsVisible"
								id="map-results"
								role="listbox"
								aria-label="Search results"
								class="absolute left-0 top-full z-20 mt-1 max-h-72 w-72 overflow-y-auto rounded-xl border border-line bg-surface-base py-1 shadow-lg"
								data-testid="map-results"
							>
								<li
									v-for="(r, n) in results"
									:id="`map-result-${n}`"
									:key="r.id"
									role="option"
									:aria-selected="n === active"
									class="flex cursor-pointer items-baseline justify-between gap-3 px-3 py-1.5 text-sm"
									:class="n === active ? 'bg-surface-hover' : ''"
									@mousedown.prevent="openResult(n)"
								>
									<span class="truncate">{{ r.label }}</span>
									<span class="shrink-0 text-xs text-ink-muted">{{
										r.cluster
									}}</span>
								</li>
							</ul>
						</div>
						<div
							class="flex rounded-xl border border-line bg-surface p-0.5"
							role="group"
							aria-label="Time range"
						>
							<button
								v-for="r in RANGES"
								:key="r.key"
								type="button"
								class="whitespace-nowrap rounded-lg px-2 py-1.5 text-xs sm:px-3 sm:text-sm"
								:class="
									range === r.key
										? 'bg-ink text-surface-base'
										: 'text-ink-muted hover:text-ink'
								"
								:aria-pressed="range === r.key"
								:data-testid="`range-${r.key}`"
								@click="range = r.key"
							>
								{{ r.label }}
							</button>
						</div>
						<button
							type="button"
							role="switch"
							:aria-checked="group"
							class="flex items-center gap-2 rounded-xl border border-line bg-surface px-3 py-2 text-sm"
							data-testid="toggle-group"
							@click="group = !group"
						>
							<span
								class="relative h-4 w-7 rounded-full transition-colors"
								:class="group ? 'bg-ink' : 'bg-line-strong'"
							>
								<span
									class="absolute top-0.5 h-3 w-3 rounded-full bg-surface-base transition-all"
									:class="group ? 'left-3.5' : 'left-0.5'"
								/>
							</span>
							Group by category
						</button>
					</div>
				</header>

				<div class="flex min-h-0 flex-1 gap-4">
					<section
						class="relative h-[calc(100dvh-300px)] min-h-[360px] min-w-0 flex-1 overflow-hidden rounded-2xl border border-line bg-surface-sunken sm:h-[calc(100dvh-250px)] sm:min-h-[420px] lg:h-[calc(100dvh-200px)] lg:min-h-[560px]"
						data-testid="map-stage"
					>
						<MapCanvas
							ref="canvas"
							:prepared="prepared"
							:group="group"
							:show-orphans="showOrphans"
							:cursor="cursorMs"
							:matches="matches"
							:pinned="pinned"
							@select="openEntity"
						/>

						<div
							v-if="notice"
							class="bg-surface/90 absolute left-3 top-3 max-w-[70%] rounded-lg border border-line px-3 py-1.5 text-xs text-ink-muted backdrop-blur"
							role="status"
							data-testid="map-truncated"
						>
							{{ notice }}
						</div>

						<div class="absolute right-3 top-3 flex flex-col gap-1.5">
							<button
								v-for="b in controls"
								:key="b.label"
								type="button"
								class="bg-surface/90 rounded-lg border border-line p-2 text-ink-muted backdrop-blur hover:text-ink"
								:aria-label="b.label"
								@click="b.run()"
							>
								<component :is="b.icon" class="h-4 w-4" />
							</button>
						</div>

						<div
							v-if="state === 'loading'"
							class="absolute inset-0 flex items-center justify-center text-sm text-ink-muted"
							role="status"
						>
							Mapping your workspace…
						</div>
						<div
							v-else-if="state === 'error'"
							class="absolute inset-0 flex flex-col items-center justify-center gap-3 text-sm"
						>
							<p class="text-ink-muted">The map could not be loaded.</p>
							<button
								type="button"
								class="rounded-lg bg-ink px-3.5 py-2 font-semibold text-surface-base"
								@click="load"
							>
								Try again
							</button>
						</div>
						<div
							v-else-if="state === 'empty'"
							class="absolute inset-0 flex flex-col items-center justify-center gap-3 px-6 text-center text-sm"
							data-testid="map-empty"
						>
							<p class="font-semibold">Nothing to map in this period</p>
							<p class="max-w-xs text-ink-muted">
								Tasks and pages show up here once they are created or updated in
								the selected time range.
							</p>
							<button
								v-if="range !== 'all'"
								type="button"
								class="rounded-lg bg-ink px-3.5 py-2 font-semibold text-surface-base"
								@click="range = 'all'"
							>
								Show all time
							</button>
						</div>

						<div
							v-if="state === 'ready'"
							class="pointer-events-none absolute inset-x-3 flex flex-col gap-2"
							:class="narrow ? 'bottom-14' : 'bottom-9'"
						>
							<MapTimeline
								v-model="step"
								class="max-w-xl"
								:steps="timeline.length"
								:label="cursorLabel"
								:playing="playing"
								@toggle="togglePlay"
							/>
						</div>
						<div
							v-if="!narrow"
							class="pointer-events-none absolute bottom-3 left-4 right-4 text-xs text-ink-muted"
							data-testid="map-legend"
						>
							{{ MAP_LEGEND }}
						</div>

						<div
							v-if="narrow"
							class="absolute inset-x-0 bottom-0 flex max-h-[62%] flex-col rounded-t-2xl border-t border-line bg-surface-base shadow-lg"
							data-testid="map-sheet"
						>
							<button
								type="button"
								class="flex shrink-0 items-center justify-between px-4 py-3 text-sm font-semibold"
								:aria-expanded="sheetOpen"
								data-testid="map-sheet-toggle"
								@click="sheetOpen = !sheetOpen"
							>
								What the map shows
								<ChevronUp
									class="h-4 w-4 transition-transform"
									:class="sheetOpen ? 'rotate-180' : ''"
								/>
							</button>
							<div v-if="sheetOpen" class="min-h-0 overflow-y-auto px-3 pb-3">
								<MapInsightsPanel
									:insights="insights"
									:show-orphans="showOrphans"
									legend
									bare
									@focus="focusIds"
									@focus-orphans="focusOrphans"
									@toggle-orphans="showOrphans = !showOrphans"
								/>
							</div>
						</div>
					</section>

					<aside v-if="!narrow" class="w-80 shrink-0 overflow-y-auto">
						<MapInsightsPanel
							:insights="insights"
							:show-orphans="showOrphans"
							@focus="focusIds"
							@focus-orphans="focusOrphans"
							@toggle-orphans="showOrphans = !showOrphans"
						/>
					</aside>
				</div>
			</div>
			<GraphOverlay
				v-if="enabled && overlayEntity"
				:entity="overlayEntity"
				@close="overlayEntity = null"
			/>
		</template>
	</BaseLayout>
</template>

<script lang="ts">
	import { getGraphMap } from '@/actions/tmgr/graph';
	import MapCanvas from '@/components/graph/map/MapCanvas.vue';
	import MapInsightsPanel from '@/components/graph/map/MapInsightsPanel.vue';
	import MapTimeline from '@/components/graph/map/MapTimeline.vue';
	import {
		buildInsights,
		endOfDayMs,
		MAP_LEGEND,
		matchNodes,
		prepareMap,
		rangeBounds,
		RANGES,
		startOfDayMs,
		timelineSpan,
		timelineSteps,
		truncationNotice,
		type PreparedMap,
		type RangeKey,
	} from '@/components/graph/map/mapLogic';
	import BaseLayout from '@/components/layouts/BaseLayout.vue';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { ChevronUp, Maximize2, Minus, Plus, Search } from 'lucide-vue-next';
	import {
		computed,
		defineAsyncComponent,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		shallowRef,
		watch,
	} from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const MAP_LIMIT = 3000;
	const PLAY_MS = 450;

	export default defineComponent({
		name: 'WorkspaceMap',
		components: {
			BaseLayout,
			MapCanvas,
			MapInsightsPanel,
			MapTimeline,
			Search,
			ChevronUp,
			GraphOverlay: defineAsyncComponent(
				() => import('@/components/graph/GraphOverlay.vue'),
			),
		},
		setup() {
			const store = useStore();
			const router = useRouter();
			const { isFeatureEnabled, isLoaded } = useFeatureToggles();
			const enabled = computed(
				() => isLoaded.value && isFeatureEnabled('graph'),
			);
			const canvas = ref<{
				fit: () => void;
				zoomBy: (factor: number) => void;
				focusIndices: (indices: number[]) => void;
			} | null>(null);
			const range = ref<RangeKey>('30');
			const query = ref('');
			const group = ref(true);
			const showOrphans = ref(true);
			const state = ref<'loading' | 'ready' | 'empty' | 'error'>('loading');
			const prepared = shallowRef<PreparedMap | null>(null);
			const timeline = ref<number[]>([0]);
			const step = ref(0);
			const playing = ref(false);
			const pinned = ref<number[] | null>(null);
			const overlayEntity = ref<string | null>(null);
			const narrow = ref(false);
			const sheetOpen = ref(false);
			let seq = 0;
			let timer: ReturnType<typeof setInterval> | undefined;
			let media: MediaQueryList | null = null;

			const workspace = computed(() => store.getters.currentWorkspace);
			const workspaceCode = computed<string>(() => workspace.value?.code ?? '');

			const insights = computed(() =>
				prepared.value && prepared.value.nodes.length
					? buildInsights(prepared.value)
					: null,
			);
			const matches = computed(() =>
				prepared.value ? matchNodes(prepared.value, query.value) : null,
			);
			const notice = computed(() =>
				prepared.value
					? truncationNotice(
							prepared.value.nodes.length,
							prepared.value.map.total,
							prepared.value.map.truncated,
					  )
					: null,
			);
			const cursorMs = computed(() => timeline.value[step.value] ?? Infinity);
			const cursorLabel = computed(() => {
				const t = cursorMs.value;
				return Number.isFinite(t)
					? new Date(t).toLocaleDateString('en', {
							month: 'short',
							day: 'numeric',
					  })
					: '';
			});

			const stopPlay = () => {
				playing.value = false;
				clearInterval(timer);
			};
			const togglePlay = () => {
				if (playing.value) return stopPlay();
				if (step.value >= timeline.value.length - 1) step.value = 0;
				playing.value = true;
				timer = setInterval(() => {
					if (step.value >= timeline.value.length - 1) return stopPlay();
					step.value++;
				}, PLAY_MS);
			};

			const load = async () => {
				const id = workspace.value?.id;
				if (!id || !enabled.value) return;
				const current = ++seq;
				stopPlay();
				state.value = 'loading';
				const bounds = rangeBounds(range.value, new Date());
				try {
					const map = await getGraphMap({
						workspace_id: id,
						from: bounds.from,
						to: bounds.to,
						limit: MAP_LIMIT,
					});
					if (current !== seq) return;
					const next = prepareMap(map, bounds);
					const span = timelineSpan(
						next.createdMs,
						bounds.from ? startOfDayMs(bounds.from) : null,
						endOfDayMs(bounds.to),
					);
					timeline.value = timelineSteps(span.start, span.end);
					step.value = timeline.value.length - 1;
					pinned.value = null;
					prepared.value = next;
					state.value = next.nodes.length ? 'ready' : 'empty';
				} catch {
					if (current === seq) state.value = 'error';
				}
			};

			const indicesOf = (ids: string[]): number[] => {
				const index = prepared.value?.index;
				if (!index) return [];
				return ids
					.map((id) => index.get(id))
					.filter((i): i is number => i !== undefined);
			};
			const focusIds = (ids: string[]) => {
				const list = indicesOf(ids);
				if (!list.length) return;
				pinned.value = list;
				canvas.value?.focusIndices(list);
				if (narrow.value) sheetOpen.value = false;
			};
			const focusOrphans = (ids: string[]) => {
				showOrphans.value = true;
				focusIds(ids);
			};
			const listOpen = ref(false);
			const active = ref(-1);
			const results = computed(() => {
				const p = prepared.value;
				if (!p || !matches.value) return [];
				return matches.value.slice(0, 8).map((i) => ({
					index: i,
					id: p.nodes[i].id,
					label: p.labels[i],
					cluster: p.clusters[p.cluster[i]].title,
				}));
			});
			const resultsVisible = computed(
				() => listOpen.value && results.value.length > 0,
			);
			const onSearchInput = () => {
				listOpen.value = true;
				active.value = -1;
			};
			const openResult = (n: number) => {
				const r = results.value[n];
				if (!r) return;
				listOpen.value = false;
				canvas.value?.focusIndices([r.index]);
				openEntity(r.id);
			};
			const onSearchKey = (e: KeyboardEvent) => {
				const count = results.value.length;
				if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
					if (!count) return;
					e.preventDefault();
					listOpen.value = true;
					const step = e.key === 'ArrowDown' ? 1 : -1;
					active.value = (active.value + step + count) % count;
					canvas.value?.focusIndices([results.value[active.value].index]);
				} else if (e.key === 'Enter') {
					e.preventDefault();
					if (active.value >= 0) openResult(active.value);
					else jumpToFirst();
				} else if (e.key === 'Escape') {
					if (resultsVisible.value) {
						listOpen.value = false;
						active.value = -1;
					} else query.value = '';
				}
			};
			const jumpToFirst = () => {
				const first = matches.value?.[0];
				if (first !== undefined) canvas.value?.focusIndices([first]);
			};
			const openEntity = (id: string) => {
				overlayEntity.value = id;
			};

			const controls = [
				{ label: 'Zoom in', icon: Plus, run: () => canvas.value?.zoomBy(1.4) },
				{
					label: 'Zoom out',
					icon: Minus,
					run: () => canvas.value?.zoomBy(1 / 1.4),
				},
				{
					label: 'Fit to screen',
					icon: Maximize2,
					run: () => canvas.value?.fit(),
				},
			];

			watch(range, load);
			watch(enabled, (on) => (on ? load() : undefined));
			watch(
				[isLoaded, enabled],
				([loaded, on]) => {
					if (loaded && !on) router.replace('/');
				},
				{ immediate: true },
			);
			watch(() => workspace.value?.id, load);

			onMounted(() => {
				media = window.matchMedia('(max-width: 1023px)');
				narrow.value = media.matches;
				media.addEventListener('change', onMedia);
				load();
			});
			const onMedia = (e: MediaQueryListEvent) => {
				narrow.value = e.matches;
			};
			onBeforeUnmount(() => {
				seq++;
				stopPlay();
				media?.removeEventListener('change', onMedia);
			});

			return {
				RANGES,
				MAP_LEGEND,
				canvas,
				range,
				query,
				group,
				showOrphans,
				state,
				prepared,
				insights,
				matches,
				notice,
				timeline,
				step,
				playing,
				pinned,
				cursorMs,
				cursorLabel,
				overlayEntity,
				narrow,
				sheetOpen,
				workspaceCode,
				enabled,
				controls,
				togglePlay,
				load,
				focusIds,
				focusOrphans,
				jumpToFirst,
				listOpen,
				active,
				results,
				resultsVisible,
				onSearchInput,
				onSearchKey,
				openResult,
				openEntity,
			};
		},
	});
</script>
