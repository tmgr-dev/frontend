<template>
	<div class="flex min-h-screen justify-center text-center text-white">
		<div class="m-auto">
			<div class="text-9xl">404</div>

			<router-link to="/" class="hover:underline">Go home</router-link>
		</div>
	</div>
</template>

<script setup>
import { onMounted } from 'vue';
import { setDocumentTitle } from '@/composable/useDocumentTitle';

onMounted(() => {
	setDocumentTitle('Page Not Found');
});
</script>
