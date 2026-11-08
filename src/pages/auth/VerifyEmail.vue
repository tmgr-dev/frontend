<template>
	<AuthLayout>
		<template #title>{{ title }}</template>

		<div
			role="status"
			:aria-busy="verify.state.value === 'verifying'"
			class="space-y-4 text-sm text-muted-foreground"
		>
			<p v-if="verify.state.value === 'verifying'">
				{{ $t('auth.verifyEmail.verifying') }}
			</p>
			<p v-else-if="verify.state.value === 'success'">
				{{ $t('auth.verifyEmail.success') }}
			</p>
			<p v-else-if="verify.state.value === 'already_verified'">
				{{ $t('auth.verifyEmail.alreadyVerified') }}
			</p>
			<p v-else-if="verify.state.value === 'expired'">
				{{ $t('auth.verifyEmail.expired') }}
			</p>
			<p v-else-if="verify.state.value === 'wrong_account'">
				{{ $t('auth.common.differentAccountHint') }}
			</p>
			<p v-else-if="verify.state.value === 'invalid'">
				{{ $t('auth.verifyEmail.invalid') }}
			</p>
			<p v-else>{{ $t('auth.common.somethingWrong') }}</p>

			<template
				v-if="
					verify.state.value === 'expired' || verify.state.value === 'invalid'
				"
			>
				<Button
					v-if="resend.state.value !== 'sent'"
					class="h-10 w-full"
					:disabled="resend.state.value === 'sending'"
					@click="resend.send"
				>
					{{ $t('auth.verifyEmail.sendNewLink') }}
				</Button>
				<p
					v-if="resend.state.value === 'sent'"
					class="text-foreground"
					data-testid="resend-sent"
				>
					{{ $t('auth.verifyEmail.resendSent') }}
				</p>
				<p v-else-if="resend.state.value === 'rate_limited'" role="alert">
					{{
						$t('auth.verifyEmail.rateLimited', {
							wait: retryAfterLabel(resend.retryAfter.value),
						})
					}}
				</p>
				<p
					v-else-if="resend.state.value === 'already_verified'"
					class="text-foreground"
				>
					{{ $t('auth.verifyEmail.resendAlreadyVerified') }}
				</p>
				<p
					v-else-if="resend.state.value === 'error'"
					role="alert"
					class="text-destructive"
				>
					{{ $t('auth.verifyEmail.resendError') }}
				</p>
			</template>

			<Button
				v-if="verify.state.value === 'wrong_account'"
				class="h-10 w-full"
				@click="signOut"
			>
				{{ $t('auth.common.signOut') }}
			</Button>
			<router-link
				v-else-if="verify.state.value !== 'verifying'"
				to="/app"
				class="block font-medium text-primary underline underline-offset-4"
				>{{ $t('auth.common.goToTmgr') }}</router-link
			>
		</div>
	</AuthLayout>
</template>

<script setup lang="ts">
	import { logout } from '@/actions/tmgr/auth';
	import {
		resendVerificationEmail,
		verifyEmail,
	} from '@/actions/tmgr/emailVerification';
	import { getUser } from '@/actions/tmgr/user';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { t } from '@/i18n';
	import { createVerifyEmail } from '@/composable/useEmailVerification';
	import store from '@/store';
	import {
		retryAfterLabel,
		saveReturnPath,
		withoutTokenQuery,
	} from '@/utils/emailVerification';
	import { computed, onMounted } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const route = useRoute();
	const router = useRouter();
	const token = String(route.query.token ?? '');

	const machine = createVerifyEmail({
		verify: verifyEmail,
		resend: resendVerificationEmail,
		refreshUser: getUser,
	});
	const verify = machine;
	const resend = machine.resend;

	const titleKeys: Record<string, string> = {
		verifying: 'auth.verifyEmail.titles.verifying',
		success: 'auth.verifyEmail.titles.success',
		already_verified: 'auth.verifyEmail.titles.already_verified',
		expired: 'auth.common.titles.linkExpired',
		invalid: 'auth.common.titles.invalidLink',
		wrong_account: 'auth.common.titles.differentAccount',
		error: 'auth.common.titles.somethingWrong',
	};
	const title = computed(() => t(titleKeys[verify.state.value]));

	async function signOut() {
		saveReturnPath(`${route.path}?token=${encodeURIComponent(token)}`);
		try {
			await logout();
		} catch {}
		await store.dispatch('logout');
		await router.push({ name: 'Login' });
	}

	onMounted(() => {
		setDocumentTitle(t('auth.verifyEmail.documentTitle'));
		router.replace({ query: withoutTokenQuery(route.query) });
		verify.run(token);
	});
</script>
