<script setup lang="ts">
import { useSidebar } from './utils';
import { useRouter } from 'vue-router';

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

