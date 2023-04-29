<template>
	<div class="overflow-hidden">
		<ContentLoader
			:primaryColor="isNightMode ? '#1a202c' : '#fff'"
			:secondaryColor="isNightMode ? '#2b3446' : '#f2f5f7'"
			:speed="0.875"
			class="task-list-loading"
			preserveAspectRatio="none"
			viewBox="0 0 500 180"
		>
			<rect height="28" rx="0" ry="0" width="500" x="0" y="0" />
			<rect height="28" rx="0" ry="0" width="500" x="0" y="31" />
			<rect height="28" rx="0" ry="0" width="500" x="0" y="62" />
			<rect height="28" rx="0" ry="0" width="500" x="0" y="93" />
			<rect height="28" rx="0" ry="0" width="500" x="0" y="124" />
		</ContentLoader>
	</div>
</template>

<script>
	import { ContentLoader } from 'vue-content-loader';

	export default {
		name: 'LoadingTasksList',
		components: {
			ContentLoader,
		},
		computed: {
			isNightMode() {
				return this.$store.getters.getColorScheme === 'dark';
			},
		},
	};
</script>

<style lang="scss" scoped>
	.task-list-loading {
		height: 593px;

		@media (max-width: 1279px) {
			height: 560px;
		}
	}
</style>
