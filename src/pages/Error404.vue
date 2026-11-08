<template>
	<div class="flex min-h-screen justify-center text-center text-ink">
		<div class="m-auto">
			<div class="text-9xl">404</div>

			<router-link to="/app" class="hover:underline">{{
				$t('public.error404.goHome')
			}}</router-link>
		</div>
	</div>
</template>

<script setup>
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';
	import { onMounted } from 'vue';

	onMounted(() => {
		setDocumentTitle(t('public.error404.documentTitle'));
	});
</script>
