<template>
	<section ref="root" data-testid="entity-graph">
		<h3 :class="headingClass">Graph</h3>
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
		<GraphOverlay v-if="open" :entity="entity" @close="open = false" />
	</section>
</template>

<script lang="ts">
	import {
		defineAsyncComponent,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';

	export default defineComponent({
		name: 'EntityGraphSection',
		components: {
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
			let observer: IntersectionObserver | null = null;

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

			return { root, visible, open };
		},
	});
</script>
