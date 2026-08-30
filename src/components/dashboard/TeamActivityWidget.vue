<template>
  <!-- CHANGES: Added min-h-64 to prevent CLS -->
  <div class="team-activity-widget min-h-64">
    <div class="team-activity-header flex items-center justify-between mb-4">
      <h3 class="text-lg font-semibold text-ink">
        Team Activity
      </h3>
      <div class="flex items-center gap-3">
        <div class="inline-flex rounded-md border border-line overflow-hidden text-xs" role="tablist" aria-label="Activity window">
          <button
            v-for="w in windows" :key="w.key" type="button" role="tab"
            :aria-selected="w.key === window"
            :class="['px-2.5 py-1', w.key === window ? 'bg-surface-sunken text-ink' : 'text-ink-faint hover:text-ink']"
            @click="emit('window-change', w.key)"
          >{{ w.label }}</button>
        </div>
        <div class="flex items-center gap-1 text-xs text-ink-subtle">
          <div class="w-2 h-2 bg-status-done rounded-full" :class="onlineCount > 0 && 'animate-pulse'"></div>
          <span>{{ onlineCount }} active now</span>
        </div>
        <Button variant="ghost" size="sm" @click="refreshTeamActivity" :disabled="loading" class="p-2" aria-label="Refresh team activity">
          <ArrowPathIcon :class="['h-4 w-4', loading && 'animate-spin']" />
        </Button>
      </div>
    </div>
    
    <div class="team-activity-content">
      <!-- Loading skeleton -->
      <template v-if="loading && (!teamActivity || !teamActivity.members || teamActivity.members.length === 0)">
        <div class="space-y-3">
          <TeamMemberSkeleton 
            v-for="i in 3" 
            :key="`skeleton-${i}`" 
          />
        </div>
      </template>
      
      <!-- Team members list -->
      <template v-else-if="teamActivity && teamActivity.members && teamActivity.members.length > 0">
        <div class="space-y-3">
          <TeamMemberItem
            v-for="member in teamActivity.members"
            :key="member.id"
            :member="member"
            @click="handleMemberClick(member)"
            @task-click="handleTaskClick"
          />
        </div>
        
        <!-- Team summary -->
        <div class="mt-4 pt-4 border-t border-line">
          <div class="grid grid-cols-5 gap-4 text-center">
            <div><div class="text-lg font-semibold tabular-nums text-ink">{{ teamActivity.total_members }}</div><div class="text-2xs uppercase tracking-wide text-ink-subtle">Total members</div></div>
            <div><div class="text-lg font-semibold tabular-nums text-status-done-fg">{{ teamActivity.active_today }}</div><div class="text-2xs uppercase tracking-wide text-ink-subtle">Active today</div></div>
            <div><div class="text-lg font-semibold tabular-nums text-status-progress-fg">{{ teamActivity.active_timers }}</div><div class="text-2xs uppercase tracking-wide text-ink-subtle">Active timers</div></div>
            <div><div class="text-lg font-semibold tabular-nums text-brand-fg">{{ formatTrackedSeconds(teamActivity.tracked_seconds) }}</div><div class="text-2xs uppercase tracking-wide text-ink-subtle">Tracked · {{ windowLabel }}</div></div>
            <div><div class="text-lg font-semibold tabular-nums text-ink">{{ teamActivity.done_count }}</div><div class="text-2xs uppercase tracking-wide text-ink-subtle">Done · {{ windowLabel }}</div></div>
          </div>
        </div>
      </template>
      
      <!-- Empty state -->
      <template v-else>
        <EmptyState
          icon="users"
          title="No team members"
          description="Invite team members to see their activity here."
          :action="{
            label: 'Invite Members',
            onClick: inviteMembers
          }"
        />
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import type { TeamMemberActivity, TeamMemberStatus, TeamActivityWindow } from '@/types/dashboard';
import { formatTrackedSeconds } from '@/utils/dashboard/teamActivityFormat';
import Button from '@/components/ui/button/Button.vue';
import EmptyState from '@/components/EmptyState.vue';
import TeamMemberItem from './TeamMemberItem.vue';
import TeamMemberSkeleton from './TeamMemberSkeleton.vue';
import { ArrowPathIcon } from '@heroicons/vue/24/outline';

interface Props {
  teamActivity: TeamMemberActivity | null;
  loading: boolean;
  window: TeamActivityWindow;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  'refresh': [];
  'member-click': [member: TeamMemberStatus];
  'task-click': [taskId: number];
  'invite-members': [];
  'window-change': [w: TeamActivityWindow];
}>();

const router = useRouter();

const windows: { key: TeamActivityWindow; label: string }[] = [
  { key: 'today', label: 'Today' },
  { key: '7d', label: '7d' },
  { key: '30d', label: '30d' },
];
const windowLabel = computed(() => windows.find(w => w.key === props.window)?.label ?? props.window);

const onlineCount = computed(() => {
  return props.teamActivity?.online_members || 0;
});

const refreshTeamActivity = () => {
  emit('refresh');
};

const handleMemberClick = (member: TeamMemberStatus) => {
  emit('member-click', member);
  // Navigate to member profile or tasks
  router.push(`/profile/${member.id}`);
};

const handleTaskClick = (taskId: number) => {
  emit('task-click', taskId);
  // Navigate to task details
  router.push(`/tasks/${taskId}`);
};

const inviteMembers = () => {
  emit('invite-members');
  router.push('/workspace/invite');
};
</script>

<style scoped>
.team-activity-widget {
  @apply rounded-card border border-line bg-surface p-6 shadow-tmgr-xs;
}

.team-activity-header {
  @apply border-b border-line pb-4 mb-4;
}

.team-activity-content {
  @apply max-h-[28rem] min-h-48 overflow-y-auto;
}

/* Custom scrollbar */
.team-activity-content::-webkit-scrollbar {
  @apply w-2;
}

.team-activity-content::-webkit-scrollbar-track {
  @apply bg-surface-sunken rounded;
}

.team-activity-content::-webkit-scrollbar-thumb {
  @apply bg-gray-300 dark:bg-gray-600 rounded hover:bg-gray-400 dark:hover:bg-gray-500;
}
</style>