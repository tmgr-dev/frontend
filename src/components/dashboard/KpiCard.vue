<template>
	<section
		:aria-label="label"
		:data-kpi="kpiKey"
		class="flex min-h-[9rem] min-w-0 flex-col rounded-card border border-line bg-surface p-4 shadow-tmgr-xs"
	>
		<div class="flex min-w-0 items-center gap-3">
			<div
				:class="[
					'flex h-9 w-9 shrink-0 items-center justify-center rounded-lg',
					toneTile,
				]"
			>
				<component :is="icon" class="h-5 w-5" aria-hidden="true" />
			</div>
			<p
				class="min-w-0 truncate text-2xs font-bold tracking-wide text-ink-subtle uppercase"
			>
				{{ label }}
			</p>
		</div>

		<div class="mt-3 flex flex-1 flex-wrap items-end gap-x-4 gap-y-2">
			<div class="min-w-0 flex-1 basis-48">
				<template v-if="loading">
					<Skeleton class="h-8 w-20" />
					<Skeleton class="mt-2 h-3 w-28" />
				</template>
				<template v-else-if="error">
					<p class="text-sm text-ink-muted">Unavailable</p>
					<Button
						type="button"
						variant="outline"
						size="sm"
						class="mt-2"
						@click="$emit('retry')"
					>
						Retry
					</Button>
				</template>
				<template v-else>
					<p class="truncate text-2xl font-semibold text-ink tabular-nums">
						{{ value ?? '—' }}
					</p>
					<p
						v-if="deltaView"
						data-kpi-delta
						class="mt-1 flex min-w-0 flex-wrap items-center gap-x-1 text-xs"
					>
						<span
							:class="[
								'inline-flex shrink-0 items-center gap-0.5 font-medium tabular-nums',
								deltaView.colorClass,
							]"
						>
							<component
								:is="deltaView.icon"
								v-if="deltaView.icon"
								class="h-3.5 w-3.5"
								aria-hidden="true"
							/>
							{{ deltaView.text }}
						</span>
						<span v-if="deltaPeriod" class="min-w-0 text-ink-subtle">
							{{ deltaPeriod }}
						</span>
					</p>
					<p v-if="hint" class="mt-1 text-xs text-ink-muted">
						{{ hint }}
					</p>
				</template>
			</div>
			<div v-if="!error && (loading || series)" class="min-w-0 flex-1 basis-32">
				<Skeleton v-if="loading" class="h-10 w-full" />
				<Sparkline v-else-if="series" :values="series" :tone="tone" />
			</div>
		</div>
	</section>
</template>

<script lang="ts">
	import Sparkline from '@/components/dashboard/Sparkline.vue';
	import Button from '@/components/ui/button/Button.vue';
	import Skeleton from '@/components/ui/skeleton/Skeleton.vue';
	import type { DashboardTone } from '@/types/dashboard';
	import { TrendingDown, TrendingUp } from 'lucide-vue-next';
	import {
		type Component,
		computed,
		defineComponent,
		type PropType,
	} from 'vue';

	const TONE_TILE: Record<DashboardTone, string> = {
		brand: 'bg-brand-bg text-brand',
		done: 'bg-status-done-bg text-status-done-fg',
		testing: 'bg-status-testing-bg text-status-testing-fg',
		progress: 'bg-status-progress-bg text-status-progress-fg',
	};

	export default defineComponent({
		name: 'KpiCard',
		components: { Button, Skeleton, Sparkline },
		props: {
			kpiKey: { type: String, required: true },
			label: { type: String, required: true },
			// lucide and heroicons export functional components, so Object alone fails the runtime prop check.
			icon: { type: [Object, Function] as PropType<Component>, required: true },
			tone: { type: String as PropType<DashboardTone>, required: true },
			value: { type: String as PropType<string | null>, default: null },
			loading: { type: Boolean, default: false },
			error: { type: Boolean, default: false },
			series: { type: Array as PropType<number[] | null>, default: null },
			delta: { type: Number as PropType<number | null>, default: null },
			deltaPeriod: { type: String, default: '' },
			hint: { type: String, default: '' },
		},
		emits: ['retry'],
		setup(props) {
			const toneTile = computed(() => TONE_TILE[props.tone]);

			const deltaView = computed(() => {
				const d = props.delta;
				if (d === null || !Number.isFinite(d)) return null;
				if (d > 0)
					return {
						icon: TrendingUp,
						text: `+${d}%`,
						colorClass: 'text-status-done-fg',
					};
				if (d < 0)
					return {
						icon: TrendingDown,
						text: `−${Math.abs(d)}%`,
						colorClass: 'text-status-fix-fg',
					};
				return { icon: null, text: '0%', colorClass: 'text-ink-muted' };
			});

			return { toneTile, deltaView };
		},
	});
</script>
