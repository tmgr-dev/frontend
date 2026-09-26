<template>
	<AuthLayout>
		<template #title>Reset your password</template>
		<template #subtitle
			>Enter your email and we'll send you a reset link.</template
		>

		<div v-if="message">
			<div
				class="rounded-lg border border-border bg-muted p-4 text-sm text-foreground"
			>
				{{ message }}
			</div>
			<Button as-child variant="outline" class="mt-4 h-10 w-full">
				<router-link to="/login">Back to Login</router-link>
			</Button>
		</div>

		<form v-else class="space-y-4" @submit.prevent="sendResetLink">
			<div class="space-y-2">
				<Label for="email" class="text-foreground">E-mail</Label>
				<Input
					v-model="email"
					id="email"
					type="email"
					:class="{ 'border-destructive': errors.email }"
					placeholder="Enter your email"
					autocomplete="email"
					class="h-10"
				/>
				<p v-if="errors.email" class="text-sm text-destructive">
					{{ errors.email[0] }}
				</p>
			</div>
			<Button
				type="submit"
				:aria-busy="isLoading"
				:disabled="isLoading"
				class="mt-2 h-10 w-full"
			>
				<span v-if="!isLoading">Send reset link</span>
				<span v-else>Sending...</span>
			</Button>
			<div class="text-center">
				<router-link
					to="/login"
					class="text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
					>Back to Login</router-link
				>
			</div>
		</form>

		<template #footer>
			Don't have an account?
			<router-link
				to="/register"
				class="font-medium text-primary underline-offset-4 hover:underline"
				>Sign up</router-link
			>
		</template>
	</AuthLayout>
</template>

<script setup lang="ts">
	import { resetPassword } from '@/actions/tmgr/auth';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { AxiosError } from 'axios';
	import { onMounted, ref } from 'vue';

	const email = ref('');

	onMounted(() => {
		setDocumentTitle('Reset Password');
	});
	const message = ref('');
	const isLoading = ref(false);
	const errors = ref({});

	async function sendResetLink() {
		if (isLoading.value) return;
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			await resetPassword({ email: email.value });

			message.value =
				"The reset link was sent. If you didn't get the email, please check your spam folder.";
		} catch (error: unknown) {
			if (error instanceof AxiosError) {
				errors.value = error.response?.data?.errors;
			}
			// Error is already presented by the form.
		} finally {
			isLoading.value = false;
		}
	}
</script>
