<template>
  <!-- CHANGES: Added min-h-64 to prevent CLS -->
  <div class="team-activity-widget min-h-64">
    <div class="team-activity-header flex items-center justify-between mb-4">
      <h3 class="text-lg font-semibold text-ink">
        Team Activity
      </h3>
      <div class="flex items-center space-x-2">
        <div class="flex items-center space-x-1 text-xs text-ink-subtle">
          <div class="w-2 h-2 bg-status-done rounded-full animate-pulse"></div>
          <span>{{ onlineCount }} online</span>
        </div>
        <Button
          variant="ghost"
          size="sm"
          @click="refreshTeamActivity"
          :disabled="loading"
          class="p-2"
          aria-label="Refresh team activity"
        >
          <ArrowPathIcon 
            :class="[
              'h-4 w-4',
              loading && 'animate-spin'
            ]" 
          />
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
          <div class="grid grid-cols-3 gap-4 text-center">
            <div>
              <div class="text-lg font-semibold tabular-nums text-ink">
                {{ teamActivity.total_members }}
              </div>
              <div class="text-2xs uppercase tracking-wide text-ink-subtle">
                Total Members
              </div>
            </div>
            <div>
              <div class="text-lg font-semibold tabular-nums text-status-done-fg">
                {{ teamActivity.online_members }}
              </div>
              <div class="text-2xs uppercase tracking-wide text-ink-subtle">
                Online Now
              </div>
            </div>
            <div>
              <div class="text-lg font-semibold tabular-nums text-status-progress-fg">
                {{ teamActivity.active_timers }}
              </div>
              <div class="text-2xs uppercase tracking-wide text-ink-subtle">
                Active Timers
              </div>
            </div>
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
import type { TeamMemberActivity, TeamMemberStatus } from '@/types/dashboard';
import Button from '@/components/ui/button/Button.vue';
import EmptyState from '@/components/EmptyState.vue';
import TeamMemberItem from './TeamMemberItem.vue';
import TeamMemberSkeleton from './TeamMemberSkeleton.vue';
import { ArrowPathIcon } from '@heroicons/vue/24/outline';

interface Props {
  teamActivity: TeamMemberActivity | null;
  loading: boolean;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  'refresh': [];
  'member-click': [member: TeamMemberStatus];
  'task-click': [taskId: number];
  'invite-members': [];
}>();

const router = useRouter();

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
  @apply max-h-80 min-h-48 overflow-y-auto;
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