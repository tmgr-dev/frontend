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
				<p>{{ $t('auth.linkConfirm.question') }}</p>
				<Button
					class="h-10 w-full"
					:disabled="confirm.state.value === 'confirming'"
					@click="confirm.run(token)"
				>
					{{ $t('auth.linkConfirm.confirm') }}
				</Button>
				<router-link
					to="/app"
					class="block text-center underline underline-offset-4"
					>{{ $t('common.actions.cancel') }}</router-link
				>
			</template>
			<template v-else>
				<p v-if="confirm.state.value === 'linked'">
					{{ $t('auth.linkConfirm.linked', { provider: providerLabel }) }}
				</p>
				<p v-else-if="confirm.state.value === 'expired'">
					{{ $t('auth.linkConfirm.expired') }}
				</p>
				<p v-else-if="confirm.state.value === 'invalid'">
					{{ $t('auth.linkConfirm.invalid') }}
				</p>
				<p v-else-if="confirm.state.value === 'wrong_account'">
					{{ $t('auth.common.differentAccountHint') }}
				</p>
				<p v-else>{{ $t('auth.common.somethingWrong') }}</p>

				<Button
					v-if="confirm.state.value === 'wrong_account'"
					class="h-10 w-full"
					@click="signOut"
				>
					{{ $t('auth.common.signOut') }}
				</Button>
				<router-link
					v-else
					to="/app"
					class="block font-medium text-primary underline underline-offset-4"
					>{{ $t('auth.common.goToTmgr') }}</router-link
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
	import { t } from '@/i18n';
	import { createLinkConfirm } from '@/composable/useEmailVerification';
	import store from '@/store';
	import { saveReturnPath, withoutTokenQuery } from '@/utils/emailVerification';
	import { computed, onMounted } from 'vue';
	import { useRoute, useRouter } from 'vue-router';

	const route = useRoute();
	const router = useRouter();
	const token = String(route.query.token ?? '');
	const confirm = createLinkConfirm({ confirm: confirmSocialLink });

	const titleKeys: Record<string, string> = {
		ready: 'auth.linkConfirm.titles.ready',
		confirming: 'auth.linkConfirm.titles.confirming',
		linked: 'auth.linkConfirm.titles.linked',
		expired: 'auth.common.titles.linkExpired',
		invalid: 'auth.common.titles.invalidLink',
		wrong_account: 'auth.common.titles.differentAccount',
		error: 'auth.common.titles.somethingWrong',
	};
	const title = computed(() => t(titleKeys[confirm.state.value]));
	const providerLabels: Record<string, string> = {
		google: 'Google',
		github: 'GitHub',
		apple: 'Apple',
	};
	const providerLabel = computed(
		() =>
			providerLabels[confirm.provider.value] ??
			t('auth.linkConfirm.thatProvider'),
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
		setDocumentTitle(t('auth.linkConfirm.documentTitle'));
		router.replace({ query: withoutTokenQuery(route.query) });
		if (!token) confirm.run('');
	});
</script>
