<template>
	<AuthLayout>
		<template #title>{{ $t('auth.register.title') }}</template>
		<template #subtitle>{{ $t('auth.register.subtitle') }}</template>

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
			<Button
				v-if="telegramPaused"
				variant="outline"
				class="h-10"
				@click="telegramPaused = false"
				>{{ $t('auth.common.tryAgain') }}</Button
			>
			<TelegramLoginWidget
				v-else
				:key="telegramWidgetKey"
				:bot-name="telegramBotName"
				callback
				widget-size="medium"
				@auth="loginWithTelegram"
				@result-dropped="telegramResultDropped"
			/>
		</div>

		<div class="my-6 flex items-center gap-3">
			<Separator class="flex-1" />
			<span class="text-xs text-muted-foreground">{{
				$t('auth.common.orContinueWithEmail')
			}}</span>
			<Separator class="flex-1" />
		</div>

		<p v-if="message" role="status" class="mb-4 text-sm text-muted-foreground">
			{{ message }}
		</p>

		<p
			v-if="failure"
			role="alert"
			data-testid="register-error"
			class="mb-4 text-sm text-destructive"
		>
			{{ failure }}
			<router-link
				v-if="emailTaken"
				to="/login"
				class="font-medium underline underline-offset-4"
				>{{ $t('auth.common.signIn') }}</router-link
			>
		</p>

		<form class="space-y-4" @submit.prevent="register">
			<div class="space-y-2">
				<Label for="name" class="text-foreground">{{
					$t('auth.register.name')
				}}</Label>
				<Input
					v-model="form.name"
					id="name"
					type="text"
					:class="{ 'border-destructive': errors.name }"
					:placeholder="$t('auth.register.namePlaceholder')"
					autocomplete="name"
					class="h-10"
				/>
				<p v-if="errors.name" class="text-sm text-destructive">
					{{ errors.name[0] }}
				</p>
			</div>
			<div class="space-y-2">
				<Label for="email" class="text-foreground">{{
					$t('auth.common.email')
				}}</Label>
				<Input
					v-model="form.email"
					id="email"
					type="email"
					:class="{ 'border-destructive': errors.email }"
					:placeholder="$t('auth.register.emailPlaceholder')"
					autocomplete="email"
					class="h-10"
				/>
				<p v-if="errors.email" class="text-sm text-destructive">
					{{ errors.email[0] }}
				</p>
			</div>
			<div class="space-y-2">
				<Label for="password" class="text-foreground">{{
					$t('auth.common.password')
				}}</Label>
				<Input
					v-model="form.password"
					id="password"
					type="password"
					:class="{ 'border-destructive': errors.password }"
					:placeholder="$t('auth.register.passwordPlaceholder')"
					autocomplete="new-password"
					class="h-10"
				/>
				<p v-if="errors.password" class="text-sm text-destructive">
					{{ errors.password[0] }}
				</p>
			</div>
			<div class="space-y-2">
				<Label for="password_confirmation" class="text-foreground">{{
					$t('auth.register.confirmPassword')
				}}</Label>
				<Input
					v-model="form.password_confirmation"
					id="password_confirmation"
					type="password"
					:class="{ 'border-destructive': errors.password_confirmation }"
					:placeholder="$t('auth.register.confirmPasswordPlaceholder')"
					autocomplete="new-password"
					class="h-10"
				/>
				<p v-if="errors.password_confirmation" class="text-sm text-destructive">
					{{ errors.password_confirmation[0] }}
				</p>
			</div>
			<Button
				type="submit"
				:aria-busy="isLoading"
				:disabled="isLoading"
				class="mt-2 h-10 w-full"
			>
				<span v-if="!isLoading">{{ $t('auth.register.submit') }}</span>
				<span v-else>{{ $t('auth.register.creating') }}</span>
			</Button>
		</form>

		<template #footer>
			<i18n-t keypath="auth.register.haveAccount" tag="span">
				<template #link>
					<router-link
						to="/login"
						class="font-medium text-primary underline-offset-4 hover:underline"
						>{{ $t('auth.common.signIn') }}</router-link
					>
				</template>
			</i18n-t>
			<div v-if="isLocalMode" class="mt-2">
				<router-link
					to="/app"
					data-testid="continue-without-account"
					class="font-medium text-primary underline-offset-4 hover:underline"
					>{{ $t('auth.common.continueWithoutAccount') }}</router-link
				>
			</div>
		</template>
	</AuthLayout>
</template>

<script setup lang="ts">
	import {
		loginTelegramWidget,
		Register,
		register as registerAction,
	} from '@/actions/tmgr/auth';
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
	import { t } from '@/i18n';
	import { isDesktopApp } from '@/utils/desktop';
	import { DesktopAuthProvider } from '@/utils/desktopAuth';
	import { createOAuthBinding } from '@/utils/oauthBinding';
	import { submitRegistration } from '@/utils/registerFlow';
	import { computed, onMounted, ref } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const router = useRouter();
	const store = useStore();
	const isDesktop = isDesktopApp();
	const isLocalMode = computed(() => store.getters.accountMode === 'local');
	const telegramBotName = import.meta.env.VITE_TELEGRAM_BOT_NAME;
	const telegramWidgetKey = ref(0);
	const telegramFailures = ref(0);
	const telegramPaused = ref(false);

	const isLoading = ref(false);
	const message = ref('');
	const failure = ref('');
	const emailTaken = ref(false);
	const errors = ref<Record<string, string[]>>({});
	const form = ref({
		name: '',
		email: '',
		password: '',
		password_confirmation: '',
	} as Register);

	async function register() {
		if (isLoading.value) return;
		errors.value = {};
		failure.value = '';
		emailTaken.value = false;
		isLoading.value = true;
		try {
			const result = await submitRegistration(form.value, registerAction);
			if (!result.ok) {
				errors.value = result.errors;
				failure.value = result.message;
				emailTaken.value = result.emailTaken;
				return;
			}
			try {
				await enterApp();
			} catch {
				failure.value = t('auth.register.appLoadFailed');
			}
		} finally {
			isLoading.value = false;
		}
	}

	async function loginWithTelegram(user: Record<string, string | number>) {
		if (isLoading.value) return;
		try {
			isLoading.value = true;
			if (!(await loginTelegramWidget(user))) {
				telegramFailed();
				return;
			}
			await enterApp();
		} catch {
			telegramFailed();
		} finally {
			isLoading.value = false;
		}
	}

	function telegramFailed() {
		message.value = t('auth.common.telegramFailed');
		// Re-render once by itself; later failures wait for a click, so a
		// widget that fails on every load can't hammer the API.
		telegramFailures.value++;
		if (telegramFailures.value > 1) telegramPaused.value = true;
		else telegramWidgetKey.value++;
	}

	function telegramResultDropped() {
		message.value = t('auth.common.telegramNotFinished');
	}

	async function enterApp() {
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
	}

	async function loginWithSocialite(platform: DesktopAuthProvider) {
		if (isDesktop) return startDesktopSocialLogin(platform);
		if (platform === 'telegram') return;
		if (platform === 'github' || platform === 'google') {
			const binding = createOAuthBinding(platform);
			document.location.href = `${
				import.meta.env.VITE_API_BASE_URL
			}auth/login/${platform}?binding=${encodeURIComponent(binding)}`;
			return;
		}
		document.location.href = `${
			import.meta.env.VITE_API_BASE_URL
		}auth/login/${platform}`;
	}

	onMounted(() => {
		setDocumentTitle(t('auth.register.documentTitle'));
	});
</script>
