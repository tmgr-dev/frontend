<template>
	<div
		class="rounded border border-gray-200 bg-gray-100 px-3 pt-3 pb-5 shadow dark:border-gray-700 dark:bg-gray-800"
	>
		<div class="flex justify-between gap-3">
			<a
				class="w-[185px] break-words font-sans text-sm font-semibold tracking-wide text-tmgr-blue dark:text-tmgr-gray"
				:href="`/${task.id}/edit`"
				@click.prevent="$store.commit('currentTaskIdForModal', task.id)"
			>
				{{ task.title }}
			</a>

			<assignee-users
				:assignees="task.assignees"
				:show-assignee-controls="false"
				avatarsClass="h-6 w-6"
			/>
		</div>

		<div class="mt-4 flex items-center justify-between">
			<span class="text-sm text-gray-600">
				{{ secondsToHumanReadableString(task.common_time) }}
			</span>

			<div>
				<category-badge
					v-if="task.category"
					:category="task.category"
					:status-id="task.status_id"
				/>
			</div>
		</div>
	</div>
</template>

<script>
	import Badge from '../general/Badge.vue';
	import TimePreparationMixin from 'src/mixins/TimePreparationMixin';
	import CategoryBadge from 'src/components/general/CategoryBadge.vue';
	import AssigneeAvatar from 'src/components/general/AssigneeAvatar.vue';
	import AssigneeUsers from 'src/components/general/AssigneeUsers.vue';

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
				return this.$store.getters.user;
			},
		},
	};
</script>
