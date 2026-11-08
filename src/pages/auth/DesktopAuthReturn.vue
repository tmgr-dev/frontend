<template>
	<AuthLayout>
		<template #title>{{ title }}</template>
		<template #subtitle>{{ subtitle }}</template>
		<Button v-if="link" as="a" :href="link" class="h-10 w-full">
			{{ $t('auth.common.openTmgr') }}
		</Button>
	</AuthLayout>
</template>

<script>
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';
	import {
		buildDesktopCallbackUrl,
		parseRelayFragment,
	} from '@/utils/desktopAuth';
	import {
		linkConfirmationNotice,
		linkConfirmationProvider,
	} from '@/utils/emailVerification';
	import { computed, defineComponent, onMounted } from 'vue';
	import { useRoute } from 'vue-router';

	export default defineComponent({
		name: 'DesktopAuthReturn',
		components: { AuthLayout, Button },
		setup() {
			const route = useRoute();
			const result = parseRelayFragment(route.hash);
			const link = result ? buildDesktopCallbackUrl(result) : null;
			const failed = !result || 'error' in result;

			const title = computed(() =>
				failed
					? t('auth.desktop.return.failedTitle')
					: t('auth.desktop.return.successTitle'),
			);
			const subtitle = computed(() => {
				if (!result) return t('auth.desktop.return.invalidLink');
				if ('error' in result) {
					const provider = linkConfirmationProvider(result.error);
					return provider
						? linkConfirmationNotice(provider)
						: t('auth.desktop.return.tryAgain');
				}
				return t('auth.desktop.return.done');
			});

			onMounted(() => {
				setDocumentTitle(t('auth.desktop.return.documentTitle'));
				history.replaceState(history.state, '', route.path);
				if (link) window.location.href = link;
			});

			return { link, title, subtitle };
		},
	});
</script>
