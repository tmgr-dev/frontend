<template>
	<div
		class="rounded border border-gray-200 bg-gray-100 px-3 pb-5 pt-3 shadow dark:border-gray-700 dark:bg-gray-800"
	>
		<div class="flex justify-between gap-3">
			<a
				class="w-44 break-words font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
				:href="`/${task.id}`"
				@click.prevent="$store.commit('setCurrentTaskIdForModal', task.id)"
			>
				{{ task.title }}
			</a>

			<AssigneeUsers
				:assignees="task.assignees"
				:show-assignee-controls="false"
				avatarsClass="h-6 w-6"
			/>
		</div>

		<div class="mt-4 flex items-center justify-between">
			<span class="text-sm text-gray-600">
				{{ secondsToHumanReadableString(task.common_time) }}
			</span>

			<CategoryBadge
				class="flex-row-reverse"
				:category="task.category"
				:status-id="task.status_id"
			/>
		</div>
	</div>
</template>

<script>
	import Badge from '../general/Badge.vue';
	import TimePreparationMixin from '@/mixins/TimePreparationMixin';
	import CategoryBadge from '@/components/general/CategoryBadge.vue';
	import AssigneeAvatar from '@/components/general/AssigneeAvatar.vue';
	import AssigneeUsers from '@/components/general/AssigneeUsers.vue';

	export default {
		mixins: [TimePreparationMixin],
		components: {
			AssigneeUsers,
			AssigneeAvatar,
			CategoryBadge,
			Badge,
		},
		props: {
			task: {
				type: Object,
				default: () => ({}),
			},
		},
		computed: {
			badgeColor() {
				const mappings = {
					Design: 'purple',
					'Feature Request': 'teal',
					Backend: 'blue',
					QA: 'green',
					default: 'black',
				};
				return mappings[this.task.type] || mappings.default;
			},
			user() {
				return this.$store.state.user;
			},
		},
	};
</script>
