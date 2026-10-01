<template>
	<AuthLayout>
		<template #title>New Password</template>
		<template #subtitle>Choose a new password for your account.</template>

		<div
			v-if="message"
			class="mb-4 rounded-lg border p-3 text-sm"
			:class="[
				errors && Object.keys(errors).length > 0
					? 'border-destructive text-destructive'
					: 'border-border bg-muted font-medium text-foreground',
			]"
		>
			{{ message }}
		</div>

		<form class="space-y-4" @submit.prevent="resetPassword">
			<div class="space-y-2">
				<Label for="password" class="text-foreground">Password</Label>
				<Input
					v-model="form.password"
					id="password"
					name="password"
					type="password"
					:class="{
						'border-destructive': (errors as any).password?.[0],
					}"
					placeholder="Password"
					autocomplete="new-password"
					class="h-10"
				/>
				<p
					v-if="(errors as any).password?.[0]"
					class="text-sm text-destructive"
				>
					{{ (errors as any).password[0] }}
				</p>
			</div>
			<div class="space-y-2">
				<Label for="password_confirmation" class="text-foreground"
					>Password confirmation</Label
				>
				<Input
					v-model="form.password_confirmation"
					id="password_confirmation"
					name="password_confirmation"
					type="password"
					:class="{
						'border-destructive':
							(errors as any)?.password_confirmation?.[0],
					}"
					placeholder="Password confirmation"
					autocomplete="new-password"
					class="h-10"
				/>
				<p
					v-if="(errors as any)?.password_confirmation?.[0]"
					class="text-sm text-destructive"
				>
					{{ (errors as any).password_confirmation[0] }}
				</p>
			</div>
			<label
				for="logout_all_sessions"
				class="flex items-start gap-2 text-sm text-foreground"
			>
				<input
					v-model="form.logout_all_sessions"
					id="logout_all_sessions"
					type="checkbox"
					class="mt-0.5 accent-primary"
					data-testid="logout-all-sessions"
				/>
				<span>
					Sign out of all devices
					<span class="block text-xs text-muted-foreground">
						Recommended if you think someone else had access to your account.
						If your email address isn't confirmed yet, every device is signed out anyway.
					</span>
				</span>
			</label>
			<Button
				type="submit"
				:disabled="isLoading"
				:aria-busy="isLoading"
				class="mt-2 h-10 w-full"
			>
				<span class="relative">
					Reset
					<loader v-if="isLoading" class="auth-loader" is-mini />
				</span>
			</Button>
		</form>

		<template #footer>
			<div class="flex flex-col items-center gap-1.5">
				<router-link
					to="/register"
					class="underline-offset-4 hover:text-foreground hover:underline"
				>
					You don't have account?
				</router-link>
				<router-link
					to="/login"
					class="font-medium text-primary underline-offset-4 hover:underline"
				>
					Login
				</router-link>
			</div>
		</template>
	</AuthLayout>
</template>

<script setup lang="ts">
	import { setNewPassword } from '@/actions/tmgr/auth';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { AxiosError } from 'axios';
	import { onBeforeMount, ref } from 'vue';
	import { useRouter } from 'vue-router';

	const router = useRouter();

	const isLoading = ref(false);
	const form = ref({
		password: '',
		password_confirmation: '',
		logout_all_sessions: true,
	});
	const message = ref('');
	const errors = ref({});
	const token = ref('');

	onBeforeMount(() => {
		setDocumentTitle('New Password');
		const params = new URLSearchParams(location.search);
		token.value = params.get('token') || '';

		if (!token.value) {
			router.push({ name: 'NotFound' });
		}
	});

	async function resetPassword() {
		if (isLoading.value) return;
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			await setNewPassword(token.value, form.value);
			message.value =
				'Your password changed now you can log in with your new password.';
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
</script>
