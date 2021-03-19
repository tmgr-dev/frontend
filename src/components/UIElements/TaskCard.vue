<template>
	<div :class="`${$color('blocksHover')} ${$color('borderMain')} shadow rounded px-3 pt-3 pb-5 border`">
		<router-link :to="`/${task.id}/edit`" class="flex justify-between">
			<p :class="`${$color('textMain')} font-semibold font-sans tracking-wide text-sm`">{{task.title}}</p>

			<img
				class="w-6 h-6 rounded-full ml-3"
				src="https://pickaface.net/gallery/avatar/unr_sample_161118_2054_ynlrg.png"
				alt="Avatar"
			>
		</router-link>
		<div class="flex mt-4 justify-between items-center">
			<span class="text-sm text-gray-600">{{task.created_at}}</span>
			<badge v-if="task.projectCategory" :color="badgeColor">{{task.projectCategory.name}}</badge>
		</div>
	</div>
</template>
<script>
import Badge from "./Badge.vue";

export default {
	components: {
		Badge
	},
	props: {
		task: {
			type: Object,
			default: () => ({})
		}
	},
	computed: {
		badgeColor() {
			const mappings = {
				Design: "purple",
				"Feature Request": "teal",
				Backend: "blue",
				QA: "green",
				default: "teal"
			};
			return mappings[this.task.type] || mappings.default;
		}
	}
};
</script>
