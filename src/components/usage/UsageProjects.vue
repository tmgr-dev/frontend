<template>
	<SettingsSection :title="`By project · ${label}`">
		<table v-if="result.lines.length" class="w-full text-sm max-sm:block">
			<thead :class="headClass">
				<tr>
					<th class="py-2 pr-3 font-medium">Project</th>
					<th class="py-2 pr-3 text-right font-medium">Cost</th>
					<th class="py-2 pr-3 text-right font-medium">Tokens</th>
					<th class="py-2 text-right font-medium">Events</th>
				</tr>
			</thead>
			<tbody class="max-sm:block">
				<tr v-for="p in result.lines" :key="p.key" :class="rowClass">
					<td :class="cell" data-label="Project">
						<span class="break-all">{{ p.label }}</span>
					</td>
					<td :class="cellNum" data-label="Cost">
						<span
							>{{ money(p.cost_usd) }}
							<span
								v-if="p.unpriced_events > 0"
								class="text-amber-700 dark:text-amber-400"
								title="Some events have no known price"
								>*</span
							></span
						>
					</td>
					<td :class="cellNum" data-label="Tokens">
						{{ count(p.tokens_total) }}
					</td>
					<td :class="cellNum" data-label="Events">{{ count(p.events) }}</td>
				</tr>
			</tbody>
		</table>
		<p v-else class="text-sm text-ink-subtle">No usage in this period.</p>
		<p v-if="result.more > 0" class="mt-3 text-xs text-ink-subtle">
			{{ result.more }} more {{ result.more === 1 ? 'project' : 'projects' }}
		</p>
	</SettingsSection>
</template>

<script lang="ts">
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import type { UsageProjectRow } from '@/usage/api';
	import { count, money } from '@/usage/format';
	import { projectLines } from '@/usage/stats';
	import { computed, defineComponent, type PropType } from 'vue';
	import { cell, cellNum, headClass, rowClass } from './tableClasses';

	export default defineComponent({
		name: 'UsageProjects',
		components: { SettingsSection },
		props: {
			projects: {
				type: Array as PropType<UsageProjectRow[]>,
				required: true,
			},
			label: { type: String, required: true },
		},
		setup(props) {
			return {
				result: computed(() => projectLines(props.projects)),
				count,
				money,
				cell,
				cellNum,
				headClass,
				rowClass,
			};
		},
	});
</script>
