<template>
	<section
		v-if="rows.length"
		class="flex flex-col gap-1"
		data-testid="task-time-entries"
	>
		<span class="text-2xs font-bold tracking-wide text-ink-subtle uppercase"
			>Terminal sessions</span
		>
		<ul
			class="divide-y divide-line overflow-hidden rounded-card border border-line bg-surface"
		>
			<li
				v-for="row in rows"
				:key="row.id"
				class="flex flex-col gap-0.5 px-3 py-2"
			>
				<div class="flex flex-wrap items-baseline gap-x-2 text-xs text-ink">
					<span class="font-semibold">{{ row.date }}</span>
					<span class="text-ink-subtle">{{ row.range }}</span>
					<span v-if="row.user" class="text-ink-subtle">{{ row.user }}</span>
					<span class="ml-auto font-mono text-ink-subtle">{{
						row.counted
					}}</span>
				</div>
				<div
					class="flex flex-wrap items-center gap-x-2 text-2xs text-ink-subtle"
				>
					<span>{{ row.source }}</span>
					<span v-if="row.partial">partly covered by other tracked time</span>
				</div>
			</li>
		</ul>
	</section>
</template>

<script setup lang="ts">
	import {
		getTaskTimeEntries,
		type TaskTimeEntry,
	} from '@/actions/tmgr/timeEntries';
	import store from '@/store';
	import { shouldLoadTimeEntries, timeEntryRows } from '@/utils/timeEntries';
	import { computed, onMounted, ref, watch } from 'vue';

	const props = defineProps<{
		taskId: number;
		workspaceId?: number;
	}>();

	const entries = ref<TaskTimeEntry[]>([]);
	let request = 0;

	const load = async () => {
		const current = ++request;
		if (!shouldLoadTimeEntries(props.taskId, props.workspaceId)) {
			entries.value = [];
			return;
		}
		try {
			const list = await getTaskTimeEntries(props.taskId);
			if (current === request) entries.value = list;
		} catch (error) {
			console.error('Failed to load time entries', error);
		}
	};

	const rows = computed(() =>
		timeEntryRows(entries.value, store.state.user?.id),
	);

	onMounted(load);
	watch(() => [props.taskId, props.workspaceId], load);
</script>
