<template>
	<div>
		<div class="flex">
			<router-link v-if="!dontPushRouter" :to="`/${task.id}/edit`" class="font-bold text-xl z-10">
				{{ task.title }}
			</router-link>
      <a href="#" @click="$emit('openTask')" v-else class="font-bold text-xl z-10">
        {{ task.title }}
      </a>
			<div class="flex items-start task-category-in-task">
				<category-badge v-if="task.category && showCategoryBadges" :category="task.category"/>
			</div>
		</div>
		<div class="flex items-start">
			<span>
				<span class="material-icons text-xl" :class="task.start_time ? 'text-green-600' : 'text-orange-600'">alarm</span>
			</span>
			<span class="text-gray-700 ml-2">{{ taskTime }}</span>
		</div>
	</div>
</template>

<script>
	import CategoryBadge from "components/UIElements/CategoryBadge";
	export default {
		name: 'TaskMeta',
		components: {CategoryBadge},
    emits: [
        'openTask'
    ],
		props: {
			task: {
				type: Object,
				required: true
			},
			dontPushRouter: {
				type: Boolean,
				required: false,
        default: false
			},
			showCategoryBadges: {
				required: false,
				type: Boolean,
				default: true
			},
			taskTime: {
				type: String,
				required: true
			}
		}
	}
</script>
