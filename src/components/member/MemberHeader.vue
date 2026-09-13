<template>
  <section class="rounded-card border border-line bg-surface p-6 shadow-tmgr-xs">
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div class="flex items-start gap-3">
        <div class="relative">
          <div class="flex h-12 w-12 items-center justify-center rounded-full bg-brand/10 text-lg font-semibold text-brand">
            {{ initials }}
          </div>
          <span
            v-if="member?.is_online"
            class="absolute -bottom-0.5 -right-0.5 h-3.5 w-3.5 rounded-full border-2 border-surface bg-green-500"
            aria-label="Active now"
          />
        </div>
        <div>
          <h1 class="text-xl font-semibold text-ink">{{ member?.name ?? '—' }}</h1>
          <p class="text-sm text-ink-subtle">
            <span class="rounded bg-surface-sunken px-1.5 py-0.5 text-xs uppercase">{{ member?.role ?? '' }}</span>
            <span class="ml-2">{{ presence }}</span>
          </p>
          <p class="mt-1 text-xs text-ink-subtle">
            {{ member?.email }}
            <span v-if="memberSince"> · member since {{ memberSince }}</span>
          </p>
        </div>
      </div>

      <div class="flex gap-1" role="group" aria-label="Time window">
        <button
          v-for="option in windows"
          :key="option.key"
          type="button"
          class="rounded px-2 py-1 text-xs"
          :class="option.key === window
            ? 'bg-brand text-white'
            : 'bg-surface-sunken text-ink-subtle hover:text-ink'"
          @click="emit('window-change', option.key)"
        >
          {{ option.label }}
        </button>
      </div>
    </div>

    <dl class="mt-5 grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
      <div v-for="tile in tiles" :key="tile.label" class="rounded-card bg-surface-sunken p-3">
        <dt class="text-xs text-ink-subtle">{{ tile.label }}</dt>
        <dd class="text-lg font-semibold text-ink">{{ tile.value }}</dd>
        <dd v-if="tile.hint" class="text-xs text-ink-subtle">{{ tile.hint }}</dd>
      </div>
    </dl>

    <div v-if="bars.length" class="mt-5">
      <p class="mb-1 text-xs text-ink-subtle">Last {{ bars.length }} days</p>
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
      v-if="member?.current_task"
      type="button"
      class="mt-5 flex w-full items-center justify-between rounded-card border border-line px-3 py-2 text-left hover:bg-surface-sunken"
      @click="emit('task-click', member.current_task.id)"
    >
      <span class="text-sm text-ink">Working on: {{ member.current_task.title }}</span>
      <span class="text-xs text-ink-subtle">timer running</span>
    </button>
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { formatTrackedSeconds, sparklineBars } from '@/utils/dashboard/teamActivityFormat';
import { percentOfTeam, relativeAge } from '@/utils/dashboard/memberPageFormat';
import type { MemberStats, TeamActivityWindow } from '@/types/dashboard';

const props = defineProps<{
  member: MemberStats | null;
  window: TeamActivityWindow;
}>();

const emit = defineEmits<{
  'window-change': [window: TeamActivityWindow];
  'task-click': [taskId: number];
}>();

const windows: Array<{ key: TeamActivityWindow; label: string }> = [
  { key: 'today', label: 'Today' },
  { key: '7d', label: '7 days' },
  { key: '30d', label: '30 days' }
];

const initials = computed(() =>
  (props.member?.name ?? '?')
    .split(' ')
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('')
);

const presence = computed(() => {
  if (props.member?.is_online) return 'Active now';
  const ago = relativeAge(props.member?.last_activity_at);

  return ago ? `Last active ${ago}` : 'No activity yet';
});

const memberSince = computed(() => {
  const since = props.member?.member_since;

  return since ? new Date(since).toLocaleDateString() : null;
});

const bars = computed(() => sparklineBars(props.member?.daily_activity ?? []));

const dayTitle = (index: number) => {
  const point = props.member?.daily_activity?.[index];

  return point ? `${point.date}: ${point.count}` : '';
};

const tiles = computed(() => {
  const m = props.member;

  if (!m) return [];

  const share = percentOfTeam(m.tracked_seconds, m.team?.tracked_seconds ?? 0);

  return [
    { label: 'Tracked', value: formatTrackedSeconds(m.tracked_seconds), hint: share ? `${share} of team` : null },
    { label: 'Done', value: m.done_count, hint: `team ${m.team?.done_count ?? 0}` },
    { label: 'Active tasks', value: m.active_tasks, hint: `${m.assigned_count} assigned · ${m.created_count} created` },
    { label: 'Comments', value: m.comments_count, hint: `on ${m.commented_tasks_count} tasks` },
    { label: 'Streak', value: `${m.streak}d`, hint: `longest ${m.longest_streak}d` },
    { label: 'Activity', value: m.activity_count, hint: `${m.active_days} active days` }
  ];
});
</script>
