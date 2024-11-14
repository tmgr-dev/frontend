<template>
	<div class="flex min-h-screen">
		<!-- Sidebar -->
		<Sidebar01>
			<template v-slot:breadcrumbs>
				<slot name="breadcrumbs"></slot>
			</template>
			<slot></slot>
		</Sidebar01>
	</div>
</template>

<script setup>
	import { computed, onBeforeMount, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';
	import store from '@/store';
	import { getDailyTasksCount } from '@/actions/tmgr/daily-tasks';
	import Sidebar01 from '@/components/general/Sidebar01.vue';

	const route = useRoute();
	const dailyRoutinesCount = ref(0);
	const showAlert = ref(false);
	const isDarkMode = ref(false);
	const isExpanded = ref(true);

	const menuItems = [
		{
			id: 1,
			name: 'List',
			path: '/',
			icon: 'list',
		},
		{
			id: 1,
			name: 'Dashboard',
			path: '/board',
			icon: 'dashboard',
		},
		{
			id: 2,
			name: 'My folders',
			path: '/projects-categories',
			icon: 'folder',
		},
		{
			id: 3,
			name: 'Daily routines',
			path: '/daily-routines',
			icon: 'schedule',
			count: dailyRoutinesCount,
		},
	];

	const teamItems = [
		{
			id: 2,
			name: 'Archive',
			path: '/archive',
			icon: 'inventory',
		},
		{
			id: 3,
			name: 'Settings',
			path: '/settings',
			icon: 'settings',
		},
	];

	onBeforeMount(async () => {
		if (store.getters.isLoggedIn) {
			dailyRoutinesCount.value = await getDailyTasksCount();
		}
	});

	const switchOn = computed({
		get() {
			return store.state.colorScheme === 'dark';
		},
		set(value) {
			isDarkMode.value = value ? 'dark' : 'default';
			store.commit('setColorScheme', isDarkMode.value);
		},
	});

	const isCurrentRoute = (path) => {
		return route.path === path;
	};

	const toggleDarkMode = () => {
		isDarkMode.value = !isDarkMode.value;
		document.documentElement.classList.toggle('dark');
	};

	const toggleSidebar = () => {
		isExpanded.value = !isExpanded.value;
	};

	// Optional: Save sidebar state to localStorage
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
	/* For handling dark mode transitions */
	.dark {
		color-scheme: dark;
	}

	@keyframes fadeIn {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.fade-enter-active,
	.fade-leave-active {
		transition: opacity 0.2s ease;
	}

	.fade-enter-from,
	.fade-leave-to {
		opacity: 0;
	}

	/* Optional: Add hover tooltips for collapsed sidebar */
	@media (max-width: 768px) {
		.tooltip {
			position: relative;
		}

		.tooltip:hover::after {
			content: attr(data-tooltip);
			position: absolute;
			left: 100%;
			top: 50%;
			transform: translateY(-50%);
			background: rgba(0, 0, 0, 0.8);
			color: white;
			padding: 4px 8px;
			border-radius: 4px;
			font-size: 12px;
			white-space: nowrap;
			z-index: 1000;
			margin-left: 8px;
		}
	}
</style>
