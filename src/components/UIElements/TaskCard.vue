<template>
	<div
		class="bg-gray-100 dark:bg-gray-800 border-gray-200 dark:border-gray-700 shadow rounded px-3 pt-3 pb-5 border"
	>
		<div class="flex justify-between">
			<a
				class="text-tmgr-blue dark:text-tmgr-gray font-semibold font-sans tracking-wide text-sm"
				:href="`/${task.id}/edit`"
				@click.prevent="$store.commit('currentTaskIdForModal', task.id)"
				>{{ task.title }}</a
			>

			<img
				alt="Avatar"
				class="w-6 h-6 rounded-full ml-3"
				src="https://pickaface.net/gallery/avatar/unr_sample_161118_2054_ynlrg.png"
			/>
		</div>
		<div class="flex mt-4 justify-between items-center">
			<span class="text-sm text-gray-600">{{
				secondsToHumanReadableString(task.common_time)
			}}</span>
			<div>
				<category-badge
					v-if="task.category"
					:category="task.category"
					:status-id="task.status_id"
				></category-badge>
			</div>
		</div>
	</div>
</template>
<script>
	import Badge from './Badge.vue';
	import TimePreparationMixin from 'src/mixins/TimePreparationMixin';
	import CategoryBadge from 'components/UIElements/CategoryBadge';

	export default {
		mixins: [TimePreparationMixin],
		components: {
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
		},
		methods: {},
	};
</script>
