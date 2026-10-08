<template>
	<AuthLayout>
		<template #title>{{ title }}</template>
		<template #subtitle>{{ subtitle }}</template>
		<Button v-if="link" as="a" :href="link" class="h-10 w-full">
			Open TMGR
		</Button>
	</AuthLayout>
</template>

<script>
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
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
				failed ? 'Sign-in did not finish' : 'You are signed in',
			);
			const subtitle = computed(() => {
				if (!result) return 'This sign-in link is not valid. Start again in TMGR.';
				if ('error' in result) {
					const provider = linkConfirmationProvider(result.error);
					return provider
						? linkConfirmationNotice(provider)
						: 'Return to TMGR and try again.';
				}
				return 'Return to TMGR to continue. You can close this tab.';
			});

			onMounted(() => {
				setDocumentTitle('TMGR sign-in');
				history.replaceState(history.state, '', route.path);
				if (link) window.location.href = link;
			});

			return { link, title, subtitle };
		},
	});
</script>
