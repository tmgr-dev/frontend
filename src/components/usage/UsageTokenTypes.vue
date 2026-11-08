<template>
	<SettingsSection
		:title="$t('agents.usage.tokenTypes.title', { label })"
		:description="$t('agents.usage.tokenTypes.description')"
	>
		<table v-if="hasUsage" class="w-full text-sm max-sm:block">
			<thead :class="headClass">
				<tr>
					<th class="py-2 pr-3 font-medium">
						{{ $t('agents.usage.tokenTypes.type') }}
					</th>
					<th class="py-2 pr-3 text-right font-medium">
						{{ $t('agents.usage.tokenTypes.tokens') }}
					</th>
					<th class="py-2 pr-3 text-right font-medium">
						{{ $t('agents.usage.tokenTypes.share') }}
					</th>
					<th class="py-2 text-right font-medium">
						{{ $t('agents.usage.tokenTypes.estCost') }}
					</th>
				</tr>
			</thead>
			<tbody class="max-sm:block">
				<tr v-for="r in split.rows" :key="r.key" :class="rowClass">
					<td :class="cell" :data-label="$t('agents.usage.tokenTypes.type')">
						{{ r.label }}
					</td>
					<td
						:class="cellNum"
						:data-label="$t('agents.usage.tokenTypes.tokens')"
					>
						{{ count(r.tokens) }}
					</td>
					<td
						:class="cellNum"
						:data-label="$t('agents.usage.tokenTypes.share')"
					>
						{{ share(r.share) }}
					</td>
					<td
						:class="cellNum"
						:data-label="$t('agents.usage.tokenTypes.estCost')"
					>
						{{ money(r.cost) }}
					</td>
				</tr>
				<tr v-if="split.noSplit !== null" :class="rowClass">
					<td :class="cell" :data-label="$t('agents.usage.tokenTypes.type')">
						{{ $t('agents.usage.tokenTypes.noSplit') }}
					</td>
					<td
						:class="cellNum"
						:data-label="$t('agents.usage.tokenTypes.tokens')"
					>
						—
					</td>
					<td
						:class="cellNum"
						:data-label="$t('agents.usage.tokenTypes.share')"
					>
						—
					</td>
					<td
						:class="cellNum"
						:data-label="$t('agents.usage.tokenTypes.estCost')"
					>
						{{ money(split.noSplit) }}
					</td>
				</tr>
			</tbody>
		</table>
		<p v-else class="text-sm text-ink-subtle">
			{{ $t('agents.usage.noUsage') }}
		</p>
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
