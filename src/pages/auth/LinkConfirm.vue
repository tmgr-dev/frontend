<template>
	<AuthLayout>
		<template #title>{{ title }}</template>

		<div
			role="status"
			:aria-busy="confirm.state.value === 'confirming'"
			class="space-y-4 text-sm text-muted-foreground"
		>
			<template
				v-if="
					confirm.state.value === 'ready' ||
					confirm.state.value === 'confirming'
				"
			>
				<p>Link this sign-in method to your tmgr.dev account?</p>
				<Button
					class="h-10 w-full"
					:disabled="confirm.state.value === 'confirming'"
					@click="confirm.run(token)"
				>
					Confirm linking
				</Button>
				<router-link
					to="/"
					class="block text-center underline underline-offset-4"
					>Cancel</router-link
				>
			</template>
			<template v-else>
				<p v-if="confirm.state.value === 'linked'">
					Account linked. Sign in with {{ providerLabel }} next time.
				</p>
				<p v-else-if="confirm.state.value === 'expired'">
					This link has expired. Sign in with your provider again to get a new one.
				</p>
				<p v-else-if="confirm.state.value === 'invalid'">
					This link is not valid. Sign in with your provider again to get a new one.
				</p>
				<p v-else-if="confirm.state.value === 'wrong_account'">
					This link is for a different account — sign out and sign in with the
					account that received the email.
				</p>
				<p v-else>Something went wrong. Please try again in a moment.</p>

				<Button
					v-if="confirm.state.value === 'wrong_account'"
					class="h-10 w-full"
					@click="signOut"
				>
					Sign out
				</Button>
				<router-link
					v-else
					to="/"
					class="block font-medium text-primary underline underline-offset-4"
					>Go to TMGR</router-link
				>
			</template>
		</div>
	</AuthLayout>
</template>

<script setup lang="ts">
	import { logout } from '@/actions/tmgr/auth';
	import { confirmSocialLink } from '@/actions/tmgr/emailVerification';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { createLinkConfirm } from '@/composable/useEmailVerification';
	import store from '@/store';
	import { saveReturnPath, withoutTokenQuery } from '@/utils/emailVerification';
	import { computed, onMounted } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const route = useRoute();
	const router = useRouter();
	const token = String(route.query.token ?? '');
	const confirm = createLinkConfirm({ confirm: confirmSocialLink });

	const titles = {
		ready: 'Link account',
		confirming: 'Linking account',
		linked: 'Account linked',
		expired: 'Link expired',
		invalid: 'Invalid link',
		wrong_account: 'Different account',
		error: 'Something went wrong',
	};
	const title = computed(() => titles[confirm.state.value]);
	const providerLabels: Record<string, string> = {
		google: 'Google',
		github: 'GitHub',
		apple: 'Apple',
	};
	const providerLabel = computed(
		() => providerLabels[confirm.provider.value] ?? 'that provider',
	);

	async function signOut() {
		saveReturnPath(`${route.path}?token=${encodeURIComponent(token)}`);
		try {
			await logout();
		} catch {}
		await store.dispatch('logout');
		await router.push({ name: 'Login' });
	}

	onMounted(() => {
		setDocumentTitle('Link account');
		router.replace({ query: withoutTokenQuery(route.query) });
		if (!token) confirm.run('');
	});
</script>
