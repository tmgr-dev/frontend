<template>
	<AuthLayout>
		<template #title>{{ title }}</template>

		<div
			role="status"
			:aria-busy="verify.state.value === 'verifying'"
			class="space-y-4 text-sm text-muted-foreground"
		>
			<p v-if="verify.state.value === 'verifying'">Confirming your email…</p>
			<p v-else-if="verify.state.value === 'success'">
				Your email address is confirmed. Thanks!
			</p>
			<p v-else-if="verify.state.value === 'already_verified'">
				This email address was already confirmed.
			</p>
			<p v-else-if="verify.state.value === 'expired'">This link has expired.</p>
			<p v-else-if="verify.state.value === 'wrong_account'">
				This link is for a different account — sign out and sign in with the
				account that received the email.
			</p>
			<p v-else-if="verify.state.value === 'invalid'">
				This link is not valid. It may have already been replaced by a newer
				one.
			</p>
			<p v-else>Something went wrong. Please try again in a moment.</p>

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
					Send a new link
				</Button>
				<p
					v-if="resend.state.value === 'sent'"
					class="text-foreground"
					data-testid="resend-sent"
				>
					A new link is on its way to your inbox.
				</p>
				<p v-else-if="resend.state.value === 'rate_limited'" role="alert">
					Too many requests. Try again in
					{{ retryAfterLabel(resend.retryAfter.value) }}.
				</p>
				<p
					v-else-if="resend.state.value === 'already_verified'"
					class="text-foreground"
				>
					Your email address is already confirmed.
				</p>
				<p
					v-else-if="resend.state.value === 'error'"
					role="alert"
					class="text-destructive"
				>
					Could not send a new link. Please try again.
				</p>
			</template>

			<Button
				v-if="verify.state.value === 'wrong_account'"
				class="h-10 w-full"
				@click="signOut"
			>
				Sign out
			</Button>
			<router-link
				v-else-if="verify.state.value !== 'verifying'"
				to="/"
				class="block font-medium text-primary underline underline-offset-4"
				>Go to TMGR</router-link
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

	const titles = {
		verifying: 'Confirming your email',
		success: 'Email confirmed',
		already_verified: 'Already confirmed',
		expired: 'Link expired',
		invalid: 'Invalid link',
		wrong_account: 'Different account',
		error: 'Something went wrong',
	};
	const title = computed(() => titles[verify.state.value]);

	async function signOut() {
		saveReturnPath(`${route.path}?token=${encodeURIComponent(token)}`);
		try {
			await logout();
		} catch {}
		await store.dispatch('logout');
		await router.push({ name: 'Login' });
	}

	onMounted(() => {
		setDocumentTitle('Confirm email');
		router.replace({ query: withoutTokenQuery(route.query) });
		verify.run(token);
	});
</script>
