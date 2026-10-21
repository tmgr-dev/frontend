<template>
	<div
		class="rounded-card border border-line bg-surface p-4 shadow-tmgr-xs sm:p-6"
		data-top-categories
	>
		<h3 class="text-base font-semibold text-ink">Top categories</h3>
		<p v-if="subtitle" class="text-xs text-ink-muted">{{ subtitle }}</p>

		<ul v-if="loading" class="mt-4 space-y-4">
			<li v-for="i in 5" :key="`skeleton-${i}`">
				<Skeleton class="h-4 w-full" />
				<Skeleton class="mt-2 h-2 w-full rounded-full" />
			</li>
		</ul>
		<p
			v-else-if="!visible.length"
			class="py-8 text-center text-sm text-ink-muted"
			data-categories-empty
		>
			No tracked time in this period
		</p>
		<ul v-else class="mt-4 space-y-4">
			<li v-for="(item, i) in visible" :key="item.key" class="min-w-0">
				<div class="flex items-center gap-2 text-sm">
					<span
						class="h-2.5 w-2.5 shrink-0 rounded-sm"
						:class="colors[i % colors.length]"
					></span>
					<span class="min-w-0 flex-1 truncate text-ink" :title="item.name">{{
						item.name
					}}</span>
					<span class="shrink-0 text-right tabular-nums text-ink-muted">{{
						formatTrackedSeconds(item.seconds)
					}}</span>
				</div>
				<div
					class="mt-1.5 h-2 overflow-hidden rounded-full bg-surface-sunken"
					aria-hidden="true"
				>
					<div
						class="h-full rounded-full"
						:class="colors[i % colors.length]"
						:style="{ width: `${barWidth(item.seconds)}%` }"
					></div>
				</div>
			</li>
		</ul>
	</div>
</template>

<script lang="ts">
	import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
	import { formatTrackedSeconds } from '@/utils/dashboard/teamActivityFormat';
	import { computed, defineComponent, type PropType } from 'vue';

	interface CategoryItem {
		key: string;
		name: string;
		seconds: number;
	}

	const colors = [
		'bg-brand',
		'bg-status-testing',
		'bg-status-progress',
		'bg-status-done',
		'bg-status-fix',
	];

	export default defineComponent({
		name: 'TopCategoriesCard',
		components: { Skeleton },
		props: {
			items: {
				type: Array as PropType<CategoryItem[]>,
				required: true,
			},
			loading: {
				type: Boolean,
				default: false,
			},
			subtitle: {
				type: String,
				default: '',
			},
		},
		setup(props) {
			const visible = computed(() => props.items.slice(0, 5));
			const max = computed(() =>
				Math.max(0, ...visible.value.map((item) => item.seconds)),
			);
			const barWidth = (seconds: number) => {
				if (seconds <= 0 || max.value <= 0) return 0;
				return Math.max(2, (seconds / max.value) * 100);
			};

			return { visible, colors, barWidth, formatTrackedSeconds };
		},
	});
</script>
