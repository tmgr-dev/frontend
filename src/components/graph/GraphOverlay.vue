<template>
	<Teleport to="body">
		<div
			ref="dialog"
			class="fixed inset-0 z-[100] flex flex-col bg-surface-base text-ink outline-none sm:p-4"
			role="dialog"
			aria-modal="true"
			aria-label="Graph"
			tabindex="-1"
			data-testid="graph-overlay"
			@keydown="trapTab"
		>
			<div
				class="mx-auto flex min-h-0 w-full max-w-[1400px] flex-1 flex-col gap-3 p-3 sm:p-0"
			>
				<header class="flex items-start justify-between gap-3">
					<div class="min-w-0">
						<button
							v-if="trail.length"
							type="button"
							class="mb-1 inline-flex items-center gap-1 text-xs text-ink-muted hover:text-ink"
							@click="back"
						>
							<ArrowLeft class="h-3.5 w-3.5" />
							Back
						</button>
						<div class="truncate text-xs text-ink-muted">
							{{ crumb }}
						</div>
						<h2 class="truncate text-lg font-semibold sm:text-xl">
							{{ centerTitle }}
						</h2>
					</div>
					<button
						type="button"
						class="rounded-lg border border-line bg-surface p-2 text-ink-muted hover:bg-surface-hover"
						aria-label="Close graph"
						data-testid="graph-close"
						@click="$emit('close')"
					>
						<X class="h-4 w-4" />
					</button>
				</header>

				<div class="flex min-h-0 flex-1 flex-col gap-3 sm:flex-row sm:gap-4">
					<div class="min-h-0 flex-1">
						<GraphNeighborhood
							mode="full"
							:entity="entity"
							:depth="depth"
							:groups="groups"
							:selected-id="selected ? selected.id : null"
							@select="selectedId = $event"
							@center="centerOn"
							@loaded="result = $event"
						/>
					</div>

					<aside
						class="flex max-h-[44%] shrink-0 flex-col gap-3 overflow-y-auto rounded-t-2xl border border-line bg-surface p-4 sm:max-h-none sm:w-80 sm:rounded-2xl"
						data-testid="graph-panel"
					>
						<template v-if="selected">
							<div class="text-xs tracking-wide text-ink-muted">SELECTED</div>
							<div class="text-base font-semibold" data-testid="graph-selected">
								{{ selectedTitle }}
							</div>
							<div
								v-if="selected.status || selected.category"
								class="flex flex-wrap gap-2 text-xs"
							>
								<span
									v-if="selected.status"
									class="rounded-md bg-violet-100 px-2 py-1 text-violet-800 dark:bg-violet-500/20 dark:text-violet-200"
									>{{ selected.status.name }}</span
								>
								<span
									v-if="selected.category"
									class="rounded-md bg-blue-100 px-2 py-1 text-blue-800 dark:bg-blue-500/20 dark:text-blue-200"
									>{{ selected.category }}</span
								>
							</div>
							<div class="mt-1 text-xs tracking-wide text-ink-muted">
								WHY IT'S CONNECTED
							</div>
							<ul
								v-if="why.length"
								class="flex flex-col gap-2 text-sm leading-5"
								data-testid="graph-why"
							>
								<li v-for="line in why.slice(0, 8)" :key="line">
									{{ line }}
								</li>
								<li v-if="why.length > 8" class="text-ink-muted">
									+{{ why.length - 8 }} more
								</li>
							</ul>
							<p v-else class="text-sm text-ink-muted">
								This is the centre of the graph.
							</p>
							<div class="mt-1 flex gap-2">
								<button
									v-if="openLabel"
									type="button"
									class="flex-1 rounded-lg bg-ink px-3 py-2.5 text-sm font-semibold text-surface-base hover:opacity-90"
									data-testid="graph-open"
									@click="openSelected"
								>
									{{ openLabel }}
								</button>
								<button
									v-if="canCenter"
									type="button"
									class="flex-1 rounded-lg border border-line bg-surface-sunken px-3 py-2.5 text-sm hover:bg-surface-hover"
									data-testid="graph-center-here"
									@click="centerOn(selected.id)"
								>
									Center here
								</button>
							</div>
						</template>
						<p v-else class="text-sm text-ink-muted">Loading…</p>

						<div class="border-t border-line pt-3">
							<div class="mb-2 text-xs tracking-wide text-ink-muted">DEPTH</div>
							<div
								class="flex gap-1 rounded-lg bg-surface-sunken p-1"
								role="group"
								aria-label="Depth"
							>
								<button
									v-for="step in [1, 2]"
									:key="step"
									type="button"
									class="flex-1 rounded-md px-2 py-2 text-sm"
									:class="
										depth === step
											? 'bg-ink font-semibold text-surface-base'
											: 'text-ink-muted hover:text-ink'
									"
									:aria-pressed="depth === step"
									@click="depth = step as 1 | 2"
								>
									{{ step === 1 ? '1 step' : '2 steps' }}
								</button>
							</div>
							<div class="mb-2 mt-3 text-xs tracking-wide text-ink-muted">
								SHOW
							</div>
							<div class="flex flex-col gap-2 text-sm">
								<label
									v-for="group in allGroups"
									:key="group.key"
									class="flex items-center gap-2.5"
								>
									<input
										type="checkbox"
										:checked="groups.includes(group.key)"
										@change="toggle(group.key)"
									/>
									{{ group.label }}
								</label>
								<label class="flex items-center gap-2.5 text-ink-muted">
									<input type="checkbox" disabled />
									Similar by meaning (later)
								</label>
							</div>
						</div>
					</aside>
				</div>
			</div>
		</div>
	</Teleport>
</template>

<script lang="ts">
	import store from '@/store';
	import type { GraphNode, GraphResult } from '@/types/graph';
	import { pageUrl } from '@/utils/pagesTree';
	import { ArrowLeft, X } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		nextTick,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';
	import { useRouter } from 'vue-router';
	import GraphNeighborhood from './GraphNeighborhood.vue';
	import {
		ALL_GROUP_KEYS,
		GRAPH_GROUPS,
		isExpandable,
		whyFor,
		type GraphGroupKey,
	} from './graphLogic';

	const FOCUSABLE =
		'button:not([disabled]), input:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

	export default defineComponent({
		name: 'GraphOverlay',
		components: { GraphNeighborhood, ArrowLeft, X },
		props: {
			entity: { type: String, required: true },
		},
		emits: ['close'],
		setup(props, { emit }) {
			const router = useRouter();
			const dialog = ref<HTMLElement | null>(null);
			const entity = ref(props.entity);
			const trail = ref<string[]>([]);
			const depth = ref<1 | 2>(2);
			const groups = ref<GraphGroupKey[]>([...ALL_GROUP_KEYS]);
			const selectedId = ref<string | null>(null);
			const result = ref<GraphResult | null>(null);
			let previousFocus: HTMLElement | null = null;

			const nodeById = (id: string | null): GraphNode | null =>
				result.value?.nodes.find((n) => n.id === id) ?? null;
			const center = computed(() => nodeById(result.value?.center ?? null));
			const selected = computed(
				() => nodeById(selectedId.value) ?? center.value,
			);
			const centerTitle = computed(
				() => center.value?.title ?? center.value?.key ?? 'Graph',
			);
			const crumb = computed(() => {
				const c = center.value;
				if (!c) return 'Graph';
				if (c.type === 'task') {
					return [c.category, c.key].filter(Boolean).join(' · ') || 'Task';
				}
				return c.type === 'page' ? 'Page' : 'Graph';
			});
			const selectedTitle = computed(() => {
				const s = selected.value;
				if (!s) return '';
				return s.key ? `${s.key} · ${s.title}` : s.title;
			});
			const why = computed(() =>
				result.value && selected.value
					? whyFor(result.value, selected.value.id)
					: [],
			);
			const openLabel = computed(() => {
				const type = selected.value?.type;
				return type === 'task'
					? 'Open task'
					: type === 'page'
					? 'Open page'
					: '';
			});
			const canCenter = computed(() => {
				const s = selected.value;
				return (
					!!s &&
					s.id !== result.value?.center &&
					['task', 'page'].includes(s.type)
				);
			});

			const centerOn = (id: string) => {
				const target = nodeById(id);
				if (id === entity.value || (target && !isExpandable(target.type)))
					return;
				trail.value.push(entity.value);
				entity.value = id;
				selectedId.value = null;
			};
			const back = () => {
				const previous = trail.value.pop();
				if (previous) {
					entity.value = previous;
					selectedId.value = null;
				}
			};
			const toggle = (key: GraphGroupKey) => {
				groups.value = groups.value.includes(key)
					? groups.value.filter((k) => k !== key)
					: [...groups.value, key];
			};
			const openSelected = () => {
				const s = selected.value;
				if (!s) return;
				const code = store.getters.currentWorkspace?.code ?? '';
				const inModal = store.state.currentTaskIdForModal != null;
				if (s.type === 'task') {
					if (inModal) store.commit('setCurrentTaskIdForModal', s.ref_id);
					else router.push(`/${code}/tasks/${s.ref_id}`);
				} else if (s.type === 'page') {
					if (inModal) store.commit('closeTaskModal');
					router.push(pageUrl(code, String(s.meta?.slug ?? s.ref_id)));
				}
				emit('close');
			};
			const onEscape = (e: KeyboardEvent) => {
				if (e.key !== 'Escape') return;
				e.preventDefault();
				e.stopImmediatePropagation();
				emit('close');
			};
			const trapTab = (e: KeyboardEvent) => {
				if (e.key !== 'Tab' || !dialog.value) return;
				const items = Array.from(
					dialog.value.querySelectorAll<HTMLElement>(FOCUSABLE),
				);
				if (!items.length) return;
				const first = items[0];
				const last = items[items.length - 1];
				if (e.shiftKey && document.activeElement === first) {
					e.preventDefault();
					last.focus();
				} else if (!e.shiftKey && document.activeElement === last) {
					e.preventDefault();
					first.focus();
				}
			};

			onMounted(async () => {
				previousFocus = document.activeElement as HTMLElement | null;
				window.addEventListener('keydown', onEscape, true);
				await nextTick();
				dialog.value?.focus();
			});
			onBeforeUnmount(() => {
				window.removeEventListener('keydown', onEscape, true);
				previousFocus?.focus?.();
			});

			return {
				dialog,
				entity,
				trail,
				depth,
				groups,
				allGroups: GRAPH_GROUPS,
				selectedId,
				selected,
				selectedTitle,
				result,
				crumb,
				centerTitle,
				why,
				openLabel,
				canCenter,
				centerOn,
				back,
				toggle,
				openSelected,
				trapTab,
			};
		},
	});
</script>
