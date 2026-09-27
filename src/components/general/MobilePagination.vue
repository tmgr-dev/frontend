<template>
	<nav
		class="flex flex-col items-center gap-3 px-1"
		aria-label="Pagination"
	>
		<div class="flex items-center gap-0.5">
			<button
				type="button"
				aria-label="Previous page"
				:disabled="current <= 1"
				class="flex h-9 w-9 items-center justify-center rounded-pill text-ink disabled:opacity-30"
				@click="$emit('page', current - 1)"
			>
				<ChevronLeft class="h-5 w-5" />
			</button>
			<template v-for="(item, index) in items" :key="index">
				<span
					v-if="item === '…'"
					class="w-6 text-center text-sm text-ink-subtle"
					>…</span
				>
				<button
					v-else
					type="button"
					:aria-current="item === current ? 'page' : undefined"
					:class="[
						'h-9 min-w-9 rounded-pill px-1.5 text-sm',
						item === current
							? 'bg-ink font-semibold text-surface'
							: 'text-ink hover:bg-surface-hover',
					]"
					@click="item !== current && $emit('page', item)"
				>
					{{ item }}
				</button>
			</template>
			<button
				type="button"
				aria-label="Next page"
				:disabled="current >= last"
				class="flex h-9 w-9 items-center justify-center rounded-pill text-ink disabled:opacity-30"
				@click="$emit('page', current + 1)"
			>
				<ChevronRight class="h-5 w-5" />
			</button>
		</div>
		<span class="text-xs text-ink-subtle">
			Showing {{ from }}–{{ to }} of {{ total }} ·
			<select
				:value="perPage"
				aria-label="Items per page"
				class="appearance-none bg-transparent text-xs text-ink-subtle underline outline-none"
				@change="
					$emit('per-page', Number(($event.target as HTMLSelectElement).value))
				"
			>
				<option v-for="size in perPageOptions" :key="size" :value="size">
					{{ size }} per page
				</option>
			</select>
		</span>
	</nav>
</template>

<script lang="ts">
	import { pageItems } from '@/utils/pageItems';
	import { ChevronLeft, ChevronRight } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'MobilePagination',
		components: { ChevronLeft, ChevronRight },
		props: {
			current: { type: Number, required: true },
			last: { type: Number, required: true },
			from: { type: Number, default: 0 },
			to: { type: Number, default: 0 },
			total: { type: Number, required: true },
			perPage: { type: Number, required: true },
			perPageOptions: {
				type: Array as PropType<number[]>,
				default: () => [10, 25, 50],
			},
		},
		emits: ['page', 'per-page'],
		setup(props) {
			return { items: computed(() => pageItems(props.current, props.last)) };
		},
	});
</script>
