<template>
	<SettingsSection
		:title="`Tokens by type · ${label}`"
		description="Cost is estimated from list prices per token type."
	>
		<table v-if="hasUsage" class="w-full text-sm max-sm:block">
			<thead :class="headClass">
				<tr>
					<th class="py-2 pr-3 font-medium">Type</th>
					<th class="py-2 pr-3 text-right font-medium">Tokens</th>
					<th class="py-2 pr-3 text-right font-medium">Share</th>
					<th class="py-2 text-right font-medium">Est. cost</th>
				</tr>
			</thead>
			<tbody class="max-sm:block">
				<tr v-for="r in split.rows" :key="r.key" :class="rowClass">
					<td :class="cell" data-label="Type">{{ r.label }}</td>
					<td :class="cellNum" data-label="Tokens">{{ count(r.tokens) }}</td>
					<td :class="cellNum" data-label="Share">{{ share(r.share) }}</td>
					<td :class="cellNum" data-label="Est. cost">{{ money(r.cost) }}</td>
				</tr>
				<tr v-if="split.noSplit !== null" :class="rowClass">
					<td :class="cell" data-label="Type">No price split</td>
					<td :class="cellNum" data-label="Tokens">—</td>
					<td :class="cellNum" data-label="Share">—</td>
					<td :class="cellNum" data-label="Est. cost">
						{{ money(split.noSplit) }}
					</td>
				</tr>
			</tbody>
		</table>
		<p v-else class="text-sm text-ink-subtle">No usage in this period.</p>
	</SettingsSection>
</template>

<script lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import type { UsageRow } from '@/usage/api';
	import { count, money, share } from '@/usage/format';
	import { tokenTypeRows } from '@/usage/stats';
	import { computed, defineComponent, type PropType } from 'vue';
	import { cell, cellNum, headClass, rowClass } from './tableClasses';

	export default defineComponent({
		name: 'UsageTokenTypes',
		components: { SettingsSection },
		props: {
			totals: { type: Object as PropType<UsageRow>, required: true },
			label: { type: String, required: true },
		},
		setup(props) {
			return {
				split: computed(() => tokenTypeRows(props.totals)),
				hasUsage: computed(
					() => props.totals.events > 0 || props.totals.tokens_total > 0,
				),
				count,
				money,
				share,
				cell,
				cellNum,
				headClass,
				rowClass,
			};
		},
	});
</script>
