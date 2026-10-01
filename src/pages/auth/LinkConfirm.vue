<template>
	<AuthLayout>
		<template #title>{{ title }}</template>

		<div
			role="status"
			:aria-busy="confirm.state.value === 'confirming'"
			class="space-y-4 text-sm text-muted-foreground"
		>
			<p v-if="confirm.state.value === 'confirming'">Linking your account…</p>
			<p v-else-if="confirm.state.value === 'linked'">
				{{ providerName }} account linked. Sign in with {{ providerName }}
				again.
			</p>
			<p v-else-if="confirm.state.value === 'expired'">
				This link has expired. Sign in with Google again to get a new one.
			</p>
			<p v-else-if="confirm.state.value === 'invalid'">
				This link is not valid. Sign in with Google again to get a new one.
			</p>
			<p v-else>Something went wrong. Please try again in a moment.</p>

			<Button
				v-if="confirm.state.value !== 'confirming'"
				class="h-10 w-full"
				@click="router.push({ name: 'Login' })"
			>
				Go to sign in
			</Button>
		</div>
	</AuthLayout>
</template>

<script setup lang="ts">
	import { confirmSocialLink } from '@/actions/tmgr/emailVerification';
	import AuthLayout from '@/components/auth/AuthLayout.vue';
	import { Button } from '@/components/ui/button';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { createLinkConfirm } from '@/composable/useEmailVerification';
	import { computed, onMounted } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const route = useRoute();
	const router = useRouter();
	const confirm = createLinkConfirm({ confirm: confirmSocialLink });

	const providerName = computed(() => {
		const provider = confirm.provider.value || 'google';
		return provider.charAt(0).toUpperCase() + provider.slice(1);
	});

	const titles = {
		confirming: 'Linking account',
		linked: 'Account linked',
		expired: 'Link expired',
		invalid: 'Invalid link',
		error: 'Something went wrong',
	};
	const title = computed(() => titles[confirm.state.value]);

	onMounted(() => {
		setDocumentTitle('Link account');
		confirm.run(String(route.query.token ?? ''));
	});
</script>
