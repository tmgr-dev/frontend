<template>
	<div>
		<a
			:href="`/${task.id}`"
			class="relative z-10 text-sm font-medium lg:text-lg"
			@click.prevent="$emit('openTask')"
		>
			{{ task.title }}
		</a>

		<div class="flex items-center gap-2">
			<span
				:class="task.start_time ? 'text-green-600' : 'text-orange-600'"
				class="material-icons text-xs sm:text-base md:text-xl"
			>
				alarm
			</span>

			<span class="text-xs text-gray-700 sm:text-base md:text-base">
				{{ taskTime }}
			</span>
		</div>
	</div>
</template>

<script>
	import CategoryBadge from 'src/components/general/CategoryBadge.vue';

	export default {
		name: 'TaskMeta',
		components: { CategoryBadge },
		emits: ['openTask'],
		props: {
			task: {
				type: Object,
				required: true,
			},
			showCategoryBadges: {
				required: false,
				type: Boolean,
				default: true,
			},
			taskTime: {
				type: String,
				required: true,
			},
		},
	};
</script>
