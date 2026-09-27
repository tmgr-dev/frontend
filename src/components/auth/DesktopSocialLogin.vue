<template>
	<div class="mt-3 space-y-3">
		<Button
			variant="outline"
			class="h-10 w-full"
			:disabled="status === 'waiting' || status === 'completing'"
			@click="start('telegram')"
		>
			<TelegramIcon class="size-4" />
			Telegram
		</Button>
		<div
			v-if="status !== 'idle'"
			role="status"
			class="rounded-md border px-3 py-2 text-sm"
			:class="
				status === 'error'
					? 'border-destructive/40 text-destructive dark:border-destructive/60'
					: 'border-border text-muted-foreground dark:border-border'
			"
		>
			<template v-if="status === 'waiting'">
				Continue in your browser. When it asks to open TMGR, allow it.
				<button
					type="button"
					class="ml-1 font-medium text-foreground underline underline-offset-4"
					@click="cancel"
				>
					Cancel
				</button>
			</template>
			<template v-else-if="status === 'completing'">Signing you in…</template>
			<template v-else>{{ error }}</template>
		</div>
	</div>
</template>

<script>
	import TelegramIcon from '@/components/icons/TelegramIcon.vue';
	import { Button } from '@/components/ui/button';
	import {
		cancelDesktopSocialLogin,
		desktopAuthError,
		desktopAuthStatus,
		startDesktopSocialLogin,
	} from '@/composable/useDesktopSocialLogin';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'DesktopSocialLogin',
		components: { Button, TelegramIcon },
		setup() {
			return {
				status: desktopAuthStatus,
				error: desktopAuthError,
				start: startDesktopSocialLogin,
				cancel: cancelDesktopSocialLogin,
			};
		},
	});
</script>
