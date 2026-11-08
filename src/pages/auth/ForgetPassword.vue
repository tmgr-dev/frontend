<template>
	<AuthLayout>
		<template #title>{{ $t('auth.forgotPassword.title') }}</template>
		<template #subtitle>{{ $t('auth.forgotPassword.subtitle') }}</template>

		<div v-if="message">
			<div
				class="rounded-lg border border-border bg-muted p-4 text-sm text-foreground"
			>
				{{ message }}
			</div>
			<Button as-child variant="outline" class="mt-4 h-10 w-full">
				<router-link to="/login">{{
					$t('auth.common.backToLogin')
				}}</router-link>
			</Button>
		</div>

		<form v-else class="space-y-4" @submit.prevent="sendResetLink">
			<div class="space-y-2">
				<Label for="email" class="text-foreground">{{
					$t('auth.common.email')
				}}</Label>
				<Input
					v-model="email"
					id="email"
					type="email"
					:class="{ 'border-destructive': errors.email }"
					:placeholder="$t('auth.forgotPassword.emailPlaceholder')"
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
				<span v-if="!isLoading">{{ $t('auth.forgotPassword.submit') }}</span>
				<span v-else>{{ $t('auth.forgotPassword.sending') }}</span>
			</Button>
			<div class="text-center">
				<router-link
					to="/login"
					class="text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
					>{{ $t('auth.common.backToLogin') }}</router-link
				>
			</div>
		</form>

		<template #footer>
			<i18n-t keypath="auth.forgotPassword.noAccount" tag="span">
				<template #link>
					<router-link
						to="/register"
						class="font-medium text-primary underline-offset-4 hover:underline"
						>{{ $t('auth.common.signUp') }}</router-link
					>
				</template>
			</i18n-t>
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
	import { t } from '@/i18n';
	import { AxiosError } from 'axios';
	import { onMounted, ref } from 'vue';

	const email = ref('');

	onMounted(() => {
		setDocumentTitle(t('auth.forgotPassword.documentTitle'));
	});
	const message = ref('');
	const isLoading = ref(false);
	const errors = ref<Record<string, string[]>>({});

	async function sendResetLink() {
		if (isLoading.value) return;
		try {
			message.value = '';
			errors.value = {};
			isLoading.value = true;
			await resetPassword({ email: email.value });

			message.value = t('auth.forgotPassword.sent');
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
