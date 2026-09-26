<template>
	<AuthLayout>
		<template #title>Create your account</template>
		<template #subtitle
			>Set up a workspace for your tasks in a minute.</template
		>

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

		<div
			v-if="telegramBotName"
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

		<form class="space-y-4" @submit.prevent="register">
			<div class="space-y-2">
				<Label for="name" class="text-foreground">Name</Label>
				<Input
					v-model="form.name"
					id="name"
					type="text"
					:class="{ 'border-destructive': errors.name }"
					placeholder="Your name"
					autocomplete="name"
					class="h-10"
				/>
				<p v-if="errors.name" class="text-sm text-destructive">
					{{ errors.name[0] }}
				</p>
			</div>
			<div class="space-y-2">
				<Label for="email" class="text-foreground">E-mail</Label>
				<Input
					v-model="form.email"
					id="email"
					type="email"
					:class="{ 'border-destructive': errors.email }"
					placeholder="your@email.com"
					autocomplete="email"
					class="h-10"
				/>
				<p v-if="errors.email" class="text-sm text-destructive">
					{{ errors.email[0] }}
				</p>
			</div>
			<div class="space-y-2">
				<Label for="password" class="text-foreground">Password</Label>
				<Input
					v-model="form.password"
					id="password"
					type="password"
					:class="{ 'border-destructive': errors.password }"
					placeholder="Create a password"
					autocomplete="new-password"
					class="h-10"
				/>
				<p v-if="errors.password" class="text-sm text-destructive">
					{{ errors.password[0] }}
				</p>
			</div>
			<div class="space-y-2">
				<Label for="password_confirmation" class="text-foreground"
					>Confirm password</Label
				>
				<Input
					v-model="form.password_confirmation"
					id="password_confirmation"
					type="password"
					placeholder="Confirm your password"
					autocomplete="new-password"
					class="h-10"
				/>
			</div>
			<Button
				type="submit"
				:aria-busy="isLoading"
				:disabled="isLoading"
				class="mt-2 h-10 w-full"
			>
				<span v-if="!isLoading">Create account</span>
				<span v-else>Creating...</span>
			</Button>
		</form>

		<template #footer>
			Already have an account?
			<router-link
				to="/login"
				class="font-medium text-primary underline-offset-4 hover:underline"
				>Sign in</router-link
			>
		</template>
	</AuthLayout>
</template>

<script setup lang="ts">
	import { Register, register as registerAction } from '@/actions/tmgr/auth';
	import { getUser, getUserSettings } from '@/actions/tmgr/user';
	import { getWorkspaceStatuses } from '@/actions/tmgr/workspaces';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import TelegramLoginWidget from '@/components/general/TelegramLoginWidget.vue';
	import AppleIcon from '@/components/icons/AppleIcon.vue';
	import GitHubIcon from '@/components/icons/GitHubIcon.vue';
	import GoogleIcon from '@/components/icons/GoogleIcon.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { Separator } from '@/components/ui/separator';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { AxiosError } from 'axios';
	import { onMounted, ref } from 'vue';
	import { useRouter } from 'vue-router';
	import { useStore } from 'vuex';

	const router = useRouter();
	const store = useStore();
	const telegramBotName = import.meta.env.VITE_TELEGRAM_BOT_NAME;
	const telegramAuthUrl = `${
		import.meta.env.VITE_API_BASE_URL
	}auth/login/telegram/redirect`;

	const isLoading = ref(false);
	const errors = ref({});
	const form = ref({
		name: '',
		email: '',
		password: '',
		password_confirmation: '',
	} as Register);

	async function register() {
		if (isLoading.value) return;
		try {
			errors.value = {};
			isLoading.value = true;
			await registerAction(form.value);
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
			}
			// Error is already presented by the form.
		} finally {
			isLoading.value = false;
		}
	}

	async function loginWithSocialite(platform: string) {
		if (platform === 'telegram') return;
		document.location.href = `${
			import.meta.env.VITE_API_BASE_URL
		}auth/login/${platform}`;
	}

	onMounted(() => {
		setDocumentTitle('Register');
	});
</script>
