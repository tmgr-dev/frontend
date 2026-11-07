<template>
	<section ref="root" data-testid="entity-graph">
		<h3 :class="headingClass">
			<button
				type="button"
				class="flex items-center gap-1 uppercase"
				:aria-expanded="expanded"
				:aria-controls="bodyId"
				data-testid="entity-graph-toggle"
				@click="toggle"
			>
				<ChevronRight
					class="h-3.5 w-3.5 transition-transform"
					:class="expanded ? 'rotate-90' : ''"
					aria-hidden="true"
				/>
				Graph
			</button>
		</h3>
		<div v-if="expanded" :id="bodyId">
			<GraphNeighborhood
				v-if="visible"
				:entity="entity"
				mode="mini"
				@expand="open = true"
			/>
			<div
				v-else
				class="h-[280px] rounded-xl border border-line bg-surface-sunken"
			/>
		</div>
		<GraphOverlay v-if="open" :entity="entity" @close="open = false" />
	</section>
</template>

<script lang="ts">
	import { ChevronRight } from 'lucide-vue-next';
	import {
		defineAsyncComponent,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';

	const STORAGE_KEY = 'tmgr:graph-section:expanded';
	let sectionSeq = 0;

	function readExpanded(): boolean {
		try {
			return localStorage.getItem(STORAGE_KEY) === '1';
		} catch {
			return false;
		}
	}

	function writeExpanded(value: boolean) {
		try {
			localStorage.setItem(STORAGE_KEY, value ? '1' : '0');
		} catch {
			/* Storage unavailable: the choice lasts until reload. */
		}
	}

	export default defineComponent({
		name: 'EntityGraphSection',
		components: {
			ChevronRight,
			GraphNeighborhood: defineAsyncComponent(
				() => import('./GraphNeighborhood.vue'),
			),
			GraphOverlay: defineAsyncComponent(() => import('./GraphOverlay.vue')),
		},
		props: {
			entity: { type: String, required: true },
			headingClass: {
				type: String,
				default:
					'mb-1.5 text-2xs font-bold uppercase tracking-wide text-ink-subtle',
			},
		},
		setup() {
			const root = ref<HTMLElement | null>(null);
			const visible = ref(false);
			const open = ref(false);
			const expanded = ref(readExpanded());
			const bodyId = `entity-graph-body-${++sectionSeq}`;
			let observer: IntersectionObserver | null = null;

			const toggle = () => {
				expanded.value = !expanded.value;
				writeExpanded(expanded.value);
			};

			onMounted(() => {
				if (typeof IntersectionObserver === 'undefined' || !root.value) {
					visible.value = true;
					return;
				}
				observer = new IntersectionObserver(
					(entries) => {
						if (entries.some((e) => e.isIntersecting)) {
							visible.value = true;
							observer?.disconnect();
						}
					},
					{ rootMargin: '120px' },
				);
				observer.observe(root.value);
			});
			onBeforeUnmount(() => observer?.disconnect());

			return { root, visible, open, expanded, bodyId, toggle };
		},
	});
</script>
