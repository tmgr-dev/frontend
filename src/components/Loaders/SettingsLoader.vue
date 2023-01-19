<template>
	<ContentLoader
		viewBox="0 0 500 340"
		:speed="0.875"
		:primaryColor="isNightMode ? '#1a202c' : '#f7f7f7'"
		:secondaryColor="isNightMode ? '#2b3446' : '#f2f5f7'"
	>
		<path
			d="M 0 304 h 500 v 36 H 0 z M 0 282 h 172 v 14 H 0 z M 0 22 h 500 v 36 H 0 z M 0 0 h 172 v 14 H 0 z M 0 222 h 500 v 36 H 0 z M 0 200 h 172 v 14 H 0 z M 0 142 h 500 v 36 H 0 z M 0 120 h 172 v 14 H 0 z M 59 75 h 163 v 23 H 59 z"
		/>
		<rect x="0" y="75" rx="12" ry="12" width="41" height="23" />
	</ContentLoader>
</template>

<script>
	import { ContentLoader } from 'vue-content-loader';

	export default {
		name: 'SettingsLoader',
		components: {
			ContentLoader,
		},
		computed: {
			isNightMode() {
				return this.$store.getters.colorScheme === 'dark';
			},
		},
	};
</script>
