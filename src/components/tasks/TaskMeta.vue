<template>
	<div>
		<div class="flex items-center">
			<router-link
				v-if="!dontPushRouter"
				:to="`/${task.id}/edit`"
				class="z-10 text-xl font-bold"
			>
				{{ task.title }}
			</router-link>

			<a
				v-else
				:href="`/${task.id}/edit`"
				class="z-10 text-xl font-bold"
				@click.prevent="$emit('openTask')"
			>
				{{ task.title }}
			</a>

			<category-badge
				v-if="task.category && showCategoryBadges"
				:category="task.category"
			/>
		</div>

		<div class="flex items-center gap-2">
			<span>
				<span
					:class="task.start_time ? 'text-green-600' : 'text-orange-600'"
					class="material-icons text-xl"
				>
					alarm
				</span>
			</span>

			<span class="text-gray-700">{{ taskTime }}</span>
		</div>
	</div>
</template>

<script>
	import CategoryBadge from 'src/components/UIElements/CategoryBadge.vue';

	export default {
		name: 'TaskMeta',
		components: { CategoryBadge },
		emits: ['openTask'],
		props: {
			task: {
				type: Object,
				required: true,
			},
			dontPushRouter: {
				type: Boolean,
				required: false,
				default: false,
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
