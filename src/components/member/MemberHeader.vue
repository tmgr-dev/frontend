<template>
	<section
		class="rounded-card border border-line bg-surface p-6 shadow-tmgr-xs"
	>
		<div class="flex flex-wrap items-start justify-between gap-4">
			<div class="flex items-start gap-3">
				<div class="relative">
					<UserAvatar
						:user-id="member?.id ?? 0"
						:name="member?.name ?? ''"
						:has-avatar="member?.has_avatar ?? false"
						:size="48"
					/>
					<span
						v-if="member?.is_online"
						class="absolute -right-0.5 -bottom-0.5 h-3.5 w-3.5 rounded-full border-2 border-surface bg-green-500"
						:aria-label="$t('workspace.memberHeader.activeNow')"
					/>
				</div>
				<div>
					<h1 class="text-xl font-semibold text-ink">
						{{ member?.name ?? '—' }}
					</h1>
					<p class="text-sm text-ink-subtle">
						<span
							class="rounded bg-surface-sunken px-1.5 py-0.5 text-xs uppercase"
							>{{ member?.role ?? '' }}</span
						>
						<span class="ml-2">{{ presence }}</span>
					</p>
					<p class="mt-1 text-xs text-ink-subtle">
						{{ member?.email }}
						<span v-if="memberSince">
							·
							{{
								$t('workspace.memberHeader.memberSince', { date: memberSince })
							}}</span
						>
					</p>
				</div>
			</div>

			<div
				class="flex gap-1"
				role="group"
				:aria-label="$t('workspace.memberHeader.timeWindow')"
			>
				<button
					v-for="option in windows"
					:key="option.key"
					type="button"
					class="rounded px-2 py-1 text-xs"
					:class="
						option.key === window
							? 'bg-brand text-on-accent'
							: 'bg-surface-sunken text-ink-subtle hover:text-ink'
					"
					@click="emit('window-change', option.key)"
				>
					{{ $t(option.labelKey) }}
				</button>
			</div>
		</div>

		<dl class="mt-5 grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
			<div
				v-for="tile in tiles"
				:key="tile.label"
				class="rounded-card bg-surface-sunken p-3"
			>
				<dt class="text-xs text-ink-subtle">{{ tile.label }}</dt>
				<dd class="text-lg font-semibold text-ink">{{ tile.value }}</dd>
				<dd v-if="tile.hint" class="text-xs text-ink-subtle">
					{{ tile.hint }}
				</dd>
			</div>
		</dl>

		<div v-if="bars.length" class="mt-5">
			<p class="mb-1 text-xs text-ink-subtle">
				{{ $t('workspace.memberHeader.lastDays', bars.length) }}
			</p>
			<div class="flex h-6 items-end gap-0.5">
				<span
					v-for="(height, index) in bars"
					:key="index"
					class="w-full rounded-sm bg-brand/60"
					:style="{ height: `${height}px` }"
					:title="dayTitle(index)"
				/>
			</div>
		</div>

		<button
			v-if="member?.current_task && showSurface('timer.member')"
			type="button"
			class="mt-5 flex w-full items-center justify-between rounded-card border border-line px-3 py-2 text-left hover:bg-surface-sunken"
			@click="emit('task-click', member.current_task.id)"
		>
			<span class="text-sm text-ink">{{
				$t('workspace.memberHeader.workingOn', {
					title: member.current_task.title,
				})
			}}</span>
			<span class="text-xs text-ink-subtle">{{
				$t('workspace.memberHeader.timerRunning')
			}}</span>
		</button>
	</section>
</template>

<script setup lang="ts">
	import { currentLocale, t } from '@/i18n';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import type { MemberStats, TeamActivityWindow } from '@/types/dashboard';
	import {
		percentOfTeam,
		relativeAge,
		thirtyDayHint,
	} from '@/utils/dashboard/memberPageFormat';
	import {
		formatTrackedSeconds,
		sparklineBars,
	} from '@/utils/dashboard/teamActivityFormat';
	import { computed } from 'vue';

	const { showSurface } = useFeatureToggles();

	const props = defineProps<{
		member: MemberStats | null;
		window: TeamActivityWindow;
	}>();

	const emit = defineEmits<{
		'window-change': [window: TeamActivityWindow];
		'task-click': [taskId: number];
	}>();

	const windows: Array<{ key: TeamActivityWindow; labelKey: string }> = [
		{ key: 'today', labelKey: 'workspace.memberHeader.windowToday' },
		{ key: '7d', labelKey: 'workspace.memberHeader.window7d' },
		{ key: '30d', labelKey: 'workspace.memberHeader.window30d' },
		{ key: 'all', labelKey: 'workspace.memberHeader.windowAll' },
	];

	const presence = computed(() => {
		if (props.member?.is_online) return t('workspace.memberHeader.activeNow');
		const ago = relativeAge(props.member?.last_activity_at);

		return ago
			? t('workspace.memberHeader.lastActive', { ago })
			: t('workspace.memberHeader.noActivity');
	});

	const memberSince = computed(() => {
		const since = props.member?.member_since;

		return since ? new Date(since).toLocaleDateString(currentLocale()) : null;
	});

	const bars = computed(() =>
		sparklineBars(props.member?.daily_activity ?? []),
	);

	const dayTitle = (index: number) => {
		const point = props.member?.daily_activity?.[index];

		return point ? `${point.date}: ${point.count}` : '';
	};

	const tiles = computed(() => {
		const m = props.member;

		if (!m) return [];

		const share = percentOfTeam(
			m.tracked_seconds,
			m.team?.tracked_seconds ?? 0,
		);

		return [
			{
				label: t('workspace.memberHeader.tracked'),
				value: formatTrackedSeconds(m.tracked_seconds),
				hint: share ? t('workspace.memberHeader.shareOfTeam', { share }) : null,
			},
			{
				label: t('workspace.memberHeader.done'),
				value: m.done_count,
				hint: t('workspace.memberHeader.teamDone', {
					n: m.team?.done_count ?? 0,
				}),
			},
			{
				label: t('workspace.memberHeader.activeTasks'),
				value: m.active_tasks,
				hint: t('workspace.memberHeader.assignedCreated', {
					assigned: m.assigned_count,
					created: m.created_count,
				}),
			},
			...(showSurface('comments.count')
				? [
						{
							label: t('workspace.memberHeader.comments'),
							value: m.comments_count,
							hint: t(
								'workspace.memberHeader.onTasks',
								m.commented_tasks_count,
							),
						},
					]
				: []),
			{
				label: t('workspace.memberHeader.streak'),
				value: t('workspace.memberHeader.streakValue', { n: m.streak }),
				hint: thirtyDayHint(
					props.window,
					t('workspace.memberHeader.longest', { n: m.longest_streak }),
				),
			},
			{
				label: t('workspace.memberHeader.activity'),
				value: m.activity_count,
				hint: thirtyDayHint(
					props.window,
					t('workspace.memberHeader.activeDays', m.active_days),
				),
			},
		];
	});
</script>
