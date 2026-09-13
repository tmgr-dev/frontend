<script setup lang="ts">
	import { useRouter } from 'vue-router';
	import { useSidebar } from './utils';

	const { isMobile, setOpenMobile } = useSidebar();
	const router = useRouter();

	router.afterEach(() => {
		if (isMobile.value) {
			setOpenMobile(false);
		}
	});
</script>

<template>
	<slot />
</template>
