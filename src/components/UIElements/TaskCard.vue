<template>
	<div :class="`${$color('blocksHover')} ${$color('borderMain')} shadow rounded px-3 pt-3 pb-5 border`">
		<div class="flex justify-between">
			<router-link :to="`/${task.id}/edit`" :class="`${$color('textMain')} font-semibold font-sans tracking-wide text-sm`">{{task.title}}</router-link>

			<img
				class="w-6 h-6 rounded-full ml-3"
				src="https://pickaface.net/gallery/avatar/unr_sample_161118_2054_ynlrg.png"
				alt="Avatar"
			>
		</div>
		<div class="flex mt-4 justify-between items-center">
			<span class="text-sm text-gray-600">{{ secondsToHumanReadableString(task.common_time) }}</span>
			<div>
				<category-badge v-if="task.category" :category="task.category"></category-badge>
			</div>
		</div>
	</div>
</template>
<script>
import Badge from "./Badge.vue";
import moment from "moment";
import TimePreparationMixin from "src/mixins/TimePreparationMixin";
import CategoryBadge from "components/UIElements/CategoryBadge";

export default {
	mixins: [
		TimePreparationMixin
	],
	components: {
		CategoryBadge,
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
				default: "black"
			};
			return mappings[this.task.type] || mappings.default;
		}
	},
	methods: {}
};
</script>
