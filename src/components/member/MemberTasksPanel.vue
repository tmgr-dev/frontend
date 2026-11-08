<template>
	<section
		class="rounded-card border border-line bg-surface p-6 shadow-tmgr-xs"
	>
		<div
			class="mb-4 flex flex-wrap gap-1"
			role="tablist"
			:aria-label="$t('workspace.memberTasksPanel.label')"
		>
			<button
				v-for="option in tabs"
				:key="option.key"
				type="button"
				role="tab"
				:aria-selected="option.key === tab"
				class="rounded px-2.5 py-1 text-xs"
				:class="
					option.key === tab
						? 'bg-brand text-on-accent'
						: 'bg-surface-sunken text-ink-subtle hover:text-ink'
				"
				@click="emit('tab-change', option.key)"
			>
				{{ $t(option.labelKey) }}
				<span v-if="option.key === tab && !(loading && !tasks.length)">
					· {{ total }}</span
				>
			</button>
		</div>

		<p
			v-if="loading && !tasks.length"
			class="py-6 text-center text-sm text-ink-subtle"
		>
			{{ $t('workspace.memberTasksPanel.loading') }}
		</p>
		<p
			v-else-if="!tasks.length"
			class="py-6 text-center text-sm text-ink-subtle"
		>
			{{ $t('workspace.memberTasksPanel.empty') }}
		</p>

		<ul v-else class="divide-y divide-line">
			<li v-for="task in tasks" :key="task.id">
				<button
					type="button"
					class="flex w-full items-center justify-between gap-3 py-2.5 text-left hover:bg-surface-sunken"
					@click="emit('task-click', task.id)"
				>
					<span class="min-w-0">
						<span class="flex items-center gap-2">
							<span
								class="h-2 w-2 shrink-0 rounded-full"
								:style="{
									backgroundColor: task.status?.color || 'var(--color-line)',
								}"
							/>
							<span class="truncate text-sm text-ink">{{ task.title }}</span>
						</span>
						<span class="mt-0.5 block text-xs text-ink-subtle">
							<span v-if="task.category && showSurface('categories.badge')"
								>{{ task.category.code }} ·
							</span>
							<span>{{
								task.status?.name ?? $t('workspace.memberTasksPanel.noStatus')
							}}</span>
							<span v-if="task.my_comments && showSurface('comments.count')">
								·
								{{
									$t('workspace.memberTasksPanel.comments', task.my_comments)
								}}</span
							>
						</span>
					</span>
					<span class="shrink-0 text-right">
						<span class="block text-sm text-ink">{{ tracked(task) }}</span>
						<span class="block text-xs text-ink-subtle">{{ age(task) }}</span>
					</span>
				</button>
			</li>
		</ul>

		<button
			v-if="hasMore"
			type="button"
			class="mt-4 w-full rounded-card border border-line py-2 text-sm text-ink-subtle hover:text-ink"
			:disabled="loading"
			@click="emit('load-more')"
		>
			{{
				loading
					? $t('workspace.memberTasksPanel.loading')
					: $t('workspace.memberTasksPanel.showMore')
			}}
		</button>
	</section>
</template>

<script setup lang="ts">
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import type { MemberTaskRow, MemberTasksTab } from '@/types/dashboard';
	import { relativeAge } from '@/utils/dashboard/memberPageFormat';
	import { formatTrackedSeconds } from '@/utils/dashboard/teamActivityFormat';

	defineProps<{
		tasks: MemberTaskRow[];
		tab: MemberTasksTab;
		total: number;
		loading: boolean;
		hasMore: boolean;
	}>();

	const emit = defineEmits<{
		'tab-change': [tab: MemberTasksTab];
		'task-click': [taskId: number];
		'load-more': [];
	}>();

	const tabs: Array<{ key: MemberTasksTab; labelKey: string }> = [
		{ key: 'touched', labelKey: 'workspace.memberTasksPanel.tabTouched' },
		{ key: 'assigned', labelKey: 'workspace.memberTasksPanel.tabAssigned' },
		{ key: 'created', labelKey: 'workspace.memberTasksPanel.tabCreated' },
		{ key: 'done', labelKey: 'workspace.memberTasksPanel.tabDone' },
	];

	const { showSurface } = useFeatureToggles();

	const tracked = (task: MemberTaskRow) =>
		task.timer_running && showSurface('timer.member')
			? `${formatTrackedSeconds(task.tracked_seconds)} ⏱`
			: formatTrackedSeconds(task.tracked_seconds);

	const age = (task: MemberTaskRow) => relativeAge(task.last_touched_at) ?? '';
</script>
