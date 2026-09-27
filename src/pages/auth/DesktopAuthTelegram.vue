<template>
	<AuthLayout>
		<template #title>Sign in to TMGR with Telegram</template>
		<template #subtitle>{{ message }}</template>
		<div v-if="canSignIn" class="flex justify-center">
			<TelegramLoginWidget
				:bot-name="telegramBotName"
				callback
				widget-size="large"
				@auth="complete"
			/>
		</div>
	</AuthLayout>
</template>

<script>
	import { completeDesktopTelegramRelay } from '@/actions/tmgr/auth';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import TelegramLoginWidget from '@/components/general/TelegramLoginWidget.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { relayReturnHash } from '@/utils/desktopAuth';
	import { defineComponent, onMounted, ref } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const TX = /^[A-Za-z0-9_-]{43}$/;

	export default defineComponent({
		name: 'DesktopAuthTelegram',
		components: { AuthLayout, TelegramLoginWidget },
		setup() {
			const route = useRoute();
			const router = useRouter();
			const telegramBotName = import.meta.env.VITE_TELEGRAM_BOT_NAME;
			const tx = typeof route.query.tx === 'string' ? route.query.tx : '';
			const canSignIn = ref(!!telegramBotName && TX.test(tx));
			const message = ref(
				canSignIn.value
					? 'Log in below, then return to the TMGR app.'
					: 'This sign-in link is not valid. Start again in TMGR.',
			);

			const complete = async (user) => {
				canSignIn.value = false;
				message.value = 'Signing you in…';
				let result;
				try {
					result = await completeDesktopTelegramRelay(tx, user);
				} catch {
					result = { error: 'telegram' };
				}
				await router.replace({
					name: 'DesktopAuthReturn',
					hash: relayReturnHash(result),
				});
			};

			onMounted(() => setDocumentTitle('TMGR sign-in'));

			return { telegramBotName, canSignIn, message, complete };
		},
	});
</script>
