<template>
	<div
		v-if="visible"
		role="status"
		data-testid="email-verify-banner"
		class="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-amber-300 bg-amber-100 px-4 py-2 text-sm text-amber-900 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-200"
	>
		<span class="min-w-0 flex-1">
			<i18n-t keypath="general.emailVerifyBanner.confirm" tag="span">
				<template #email>
					<strong>{{ user.email }}</strong>
				</template>
			</i18n-t>
			{{ ' ' }}
			<span v-if="resend.state.value === 'sent'" class="font-medium">
				{{ $t('general.emailVerifyBanner.sent') }}
			</span>
			<span v-else-if="resend.state.value === 'rate_limited'">
				{{
					$t('general.emailVerifyBanner.rateLimited', {
						wait: retryAfterLabel(resend.retryAfter.value),
					})
				}}
			</span>
			<span v-else-if="resend.state.value === 'error'">
				{{ $t('general.emailVerifyBanner.error') }}
			</span>
		</span>
		<button
			type="button"
			class="rounded-md px-2 py-1 font-medium underline underline-offset-4 hover:bg-amber-200 disabled:opacity-60 dark:hover:bg-amber-500/20"
			:disabled="resend.state.value === 'sending'"
			@click="resend.send"
		>
			{{ $t('general.emailVerifyBanner.resend') }}
		</button>
		<button
			type="button"
			class="rounded-md px-2 py-1 hover:bg-amber-200 dark:hover:bg-amber-500/20"
			:aria-label="$t('general.emailVerifyBanner.dismiss')"
			@click="dismiss"
		>
			<X class="h-4 w-4" />
		</button>
	</div>
</template>

<script setup lang="ts">
	import { resendVerificationEmail } from '@/actions/tmgr/emailVerification';
	import { getUser, User } from '@/actions/tmgr/user';
	import { createResendController } from '@/composable/useEmailVerification';
	import store from '@/store';
	import {
		dismissBanner,
		isBannerDismissed,
		retryAfterLabel,
		shouldShowVerifyBanner,
	} from '@/utils/emailVerification';
	import { X } from 'lucide-vue-next';
	import { computed, ref } from 'vue';

	const dismissed = ref(isBannerDismissed());
	const user = computed(() => store.state.user as User);
	const visible = computed(
		() =>
			store.getters.isLoggedIn &&
			shouldShowVerifyBanner(user.value, dismissed.value),
	);

	const resend = createResendController({
		resend: resendVerificationEmail,
		onAlreadyVerified: () => getUser().catch(() => undefined),
	});

	function dismiss() {
		dismissBanner();
		dismissed.value = true;
	}
</script>
