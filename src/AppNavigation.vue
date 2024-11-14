<template>
	<div class="flex min-h-screen">
		<Sidebar01>
			<template v-slot:breadcrumbs>
				<slot name="breadcrumbs"></slot>
			</template>
			<slot></slot>
		</Sidebar01>
	</div>
</template>

<script setup>
	import { onBeforeMount, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';
	import store from '@/store';
	import { getDailyTasksCount } from '@/actions/tmgr/daily-tasks';
	import Sidebar01 from '@/components/general/Sidebar01.vue';

	const route = useRoute();
	const dailyRoutinesCount = ref(0);
	const showAlert = ref(false);
	const isExpanded = ref(true);

	onBeforeMount(async () => {
		if (store.getters.isLoggedIn) {
			dailyRoutinesCount.value = await getDailyTasksCount();
		}
	});

	if (typeof window !== 'undefined') {
		const savedState = localStorage.getItem('sidebarExpanded');
		if (savedState !== null) {
			isExpanded.value = savedState === 'true';
		}

		watch(isExpanded, (newValue) => {
			localStorage.setItem('sidebarExpanded', newValue.toString());
		});
	}
</script>

<style>
	@keyframes fadeIn {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}
</style>
