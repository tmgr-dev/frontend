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
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import TelegramLoginWidget from '@/components/general/TelegramLoginWidget.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { desktopCompleteUrl } from '@/utils/desktopAuth';
	import { defineComponent, onMounted, ref } from 'vue';
	import { useRoute } from 'vue-router';

	const TX = /^[A-Za-z0-9_-]{43}$/;

	export default defineComponent({
		name: 'DesktopAuthTelegram',
		components: { AuthLayout, TelegramLoginWidget },
		setup() {
			const route = useRoute();
			const telegramBotName = import.meta.env.VITE_TELEGRAM_BOT_NAME;
			const tx = typeof route.query.tx === 'string' ? route.query.tx : '';
			const canSignIn = ref(!!telegramBotName && TX.test(tx));
			const message = ref(
				canSignIn.value
					? 'Log in below, then return to the TMGR app.'
					: 'This sign-in link is not valid. Start again in TMGR.',
			);

			// A top-level form POST: the API accepts it only from this site's origin.
			const complete = (user) => {
				canSignIn.value = false;
				message.value = 'Signing you in…';
				const form = document.createElement('form');
				form.method = 'POST';
				form.action = desktopCompleteUrl(
					import.meta.env.VITE_API_BASE_URL,
					'telegram',
				);
				Object.entries({ ...user, tx }).forEach(([name, value]) => {
					const input = document.createElement('input');
					input.type = 'hidden';
					input.name = name;
					input.value = String(value);
					form.appendChild(input);
				});
				document.body.appendChild(form);
				form.submit();
			};

			onMounted(() => setDocumentTitle('TMGR sign-in'));

			return { telegramBotName, canSignIn, message, complete };
		},
	});
</script>
