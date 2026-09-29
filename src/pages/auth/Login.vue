<template>
	<AuthLayout>
		<template #title>Welcome back</template>
		<template #subtitle>Sign in to pick up where you left off.</template>

		<div class="grid grid-cols-3 gap-2">
			<Button
				variant="outline"
				class="h-10"
				@click="loginWithSocialite('google')"
			>
				<GoogleIcon />
				Google
			</Button>
			<Button
				variant="outline"
				class="h-10 [&_svg]:fill-current"
				@click="loginWithSocialite('apple')"
			>
				<AppleIcon />
				Apple
			</Button>
			<Button
				variant="outline"
				class="h-10 [&_svg]:fill-current"
				@click="loginWithSocialite('github')"
			>
				<GitHubIcon />
				GitHub
			</Button>
		</div>

		<DesktopSocialLogin v-if="isDesktop" />
		<div
			v-else-if="telegramBotName"
			id="telegram-register-widget-container"
			class="mt-3 flex justify-center"
		>
			<TelegramLoginWidget
				:bot-name="telegramBotName"
				:auth-url="telegramAuthUrl"
				widget-size="medium"
			/>
		</div>

		<div class="my-6 flex items-center gap-3">
			<Separator class="flex-1" />
			<span class="text-xs text-muted-foreground">Or continue with email</span>
			<Separator class="flex-1" />
		</div>

		<p v-if="message" role="status" class="mb-4 text-sm text-muted-foreground">
			{{ message }}
		</p>

		<form class="space-y-4" @submit.prevent="login">
			<div class="space-y-2">
				<Label for="email" class="text-foreground">E-mail</Label>
				<Input
					v-model="form.email"
					id="email"
					type="email"
					placeholder="you@company.com"
					autocomplete="email"
					class="h-10"
				/>
			</div>
			<div class="space-y-2">
				<div class="flex items-center justify-between">
					<Label for="password" class="text-foreground">Password</Label>
					<router-link
						to="/password/forget"
						class="text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
						>Forgot your password?</router-link
					>
				</div>
				<Input
					v-model="form.password"
					id="password"
					type="password"
					placeholder="Password"
					autocomplete="current-password"
					class="h-10"
				/>
			</div>
			<Button
				type="submit"
				:disabled="isLoading"
				:aria-busy="isLoading"
				class="mt-2 h-10 w-full"
			>
				{{ isLoading ? 'Signing in…' : 'Sign in' }}
			</Button>
		</form>

		<template #footer>
			Have no account?
			<router-link
				to="/register"
				class="font-medium text-primary underline-offset-4 hover:underline"
				>Sign up</router-link
			>
		</template>
	</AuthLayout>
</template>

<script setup lang="ts">
	import { LoginRequest, login as loginAction } from '@/actions/tmgr/auth';
	import { getUser, getUserSettings } from '@/actions/tmgr/user';
	import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import DesktopSocialLogin from '@/components/auth/DesktopSocialLogin.vue';
	import TelegramLoginWidget from '@/components/general/TelegramLoginWidget.vue';
	import AppleIcon from '@/components/icons/AppleIcon.vue';
	import GitHubIcon from '@/components/icons/GitHubIcon.vue';
	import GoogleIcon from '@/components/icons/GoogleIcon.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { Separator } from '@/components/ui/separator';
	import { startDesktopSocialLogin } from '@/composable/useDesktopSocialLogin';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { DesktopAuthProvider } from '@/utils/desktopAuth';
	import { consumeSessionExpired } from '@/utils/sessionExpiry';
	import { AxiosError } from 'axios';
	import { onMounted, ref } from 'vue';
	import { useRouter } from 'vue-router';

	const router = useRouter();
	const isDesktop = isDesktopApp();
	const telegramBotName = import.meta.env.VITE_TELEGRAM_BOT_NAME;
	const telegramAuthUrl = `${
		import.meta.env.VITE_API_BASE_URL
	}auth/login/telegram/redirect`;

	const form = ref({
		email: '',
		password: '',
	} as LoginRequest);

	const isLoading = ref(false);
	const message = ref('');
	const errors = ref({});

	async function login() {
		if (isLoading.value) return;
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			await loginAction(form.value);
			await getUser();

			if (localStorage.getItem('workspace.invitation')) {
				const token = localStorage.getItem('workspace.invitation');
				await router.push({
					name: 'WorkspaceInvitation',
					params: {
						token,
					},
				});
			} else {
				if (store.state.user) {
					await Promise.all([
						getUserSettings(),
						getWorkspaceStatuses(),
						store.dispatch('featureToggles/loadUserToggles'),
					]);
				}

				const landingPage =
					store.getters['featureToggles/getUserFeatureValue'](
						'default_landing_page',
					) || 'list';
				const currentWorkspaceId = store.state.user?.settings?.find(
					(s) => s.key === 'current_workspace',
				)?.value;
				const workspace = store.state.workspaces?.find(
					(w) => w.id == currentWorkspaceId,
				);
				const workspaceCode = workspace?.code;

				if (workspaceCode) {
					await router.push(`/${workspaceCode}/${landingPage}`);
				} else {
					await router.push({ name: 'CurrentTasksList' });
				}
			}

			store.commit('rerenderApp');
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;
				message.value = error.response?.data?.message;
			}
			// Error is already presented by the form.
		} finally {
			isLoading.value = false;
		}
	}

	async function loginWithSocialite(platform: DesktopAuthProvider) {
		if (isDesktop) return startDesktopSocialLogin(platform);
		if (platform === 'telegram') return;
		document.location.href = `${
			import.meta.env.VITE_API_BASE_URL
		}auth/login/${platform}`;
	}

	onMounted(() => {
		setDocumentTitle('Login');
		if (consumeSessionExpired()) {
			message.value = 'Your session has expired. Please sign in again.';
		}
		if (document.getElementById('telegram-login-widget-container')) {
			const script = document.createElement('script');
			script.async = true;
			script.src = 'https://telegram.org/js/telegram-widget.js?22';
			script.setAttribute('data-telegram-login', telegramBotName || '');
			script.setAttribute('data-size', 'medium');
			script.setAttribute('data-auth-url', telegramAuthUrl);
			script.setAttribute('data-request-access', 'write');
			document
				.getElementById('telegram-login-widget-container')
				?.appendChild(script);
		}
	});
</script>
