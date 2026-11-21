<template>
	<div :class="{ stale }">
		<div class="lh">
			<b>{{ label }}</b>
			<span>
				{{ pct }}%<template v-if="reset">
					· {{ $t('popover.usage.resets', { time: reset }) }}</template
				>
			</span>
		</div>
		<div v-if="stale" class="upd">
			{{ $t('popover.usage.updated', { ago: age }) }}
		</div>
		<div class="bar">
			<i
				:class="{ ok: level === 'ok', crit: level === 'crit' }"
				:style="{ width: pct + '%' }"
			></i>
		</div>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import {
		durationText,
		isStale,
		resetText,
		usageBarPct,
		usageLevel,
	} from '../model';
	import type { UsageBar } from '../types';

	export default defineComponent({
		props: {
			label: { type: String, required: true },
			bar: { type: Object as PropType<UsageBar>, required: true },
			now: { type: Number, required: true },
		},
		computed: {
			pct(): number {
				return usageBarPct(this.bar);
			},
			level() {
				return usageLevel(this.pct);
			},
			stale(): boolean {
				return isStale(this.bar);
			},
			age(): string {
				return durationText(this.bar.ageS ?? 0);
			},
			reset(): string | null {
				return resetText(this.bar.resetsAt, this.now);
			},
		},
	});
</script>
