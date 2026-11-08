<template>
	<AuthLayout>
		<template #title>{{ $t('auth.desktop.telegram.title') }}</template>
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
	import { t } from '@/i18n';
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
					? t('auth.desktop.telegram.prompt')
					: t('auth.desktop.telegram.invalidLink'),
			);

			// A top-level form POST: the API accepts it only from this site's origin.
			const complete = (user) => {
				canSignIn.value = false;
				message.value = t('auth.desktop.social.signingIn');
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

			onMounted(() => setDocumentTitle(t('auth.desktop.return.documentTitle')));

			return { telegramBotName, canSignIn, message, complete };
		},
	});
</script>
