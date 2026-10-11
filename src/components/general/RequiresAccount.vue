<template>
	<div
		:class="
			compact
				? 'flex flex-col gap-0.5 text-xs text-ink-subtle sm:items-end sm:text-right'
				: 'flex flex-col items-center gap-3 rounded-lg border border-border bg-card p-8 text-center text-card-foreground'
		"
		data-testid="requires-account"
	>
		<template v-if="compact">
			<span class="flex flex-wrap items-center gap-2 sm:justify-end">
				<span class="font-medium text-ink">Requires an account</span>
				<Button variant="outline" size="sm" @click="createAccount">
					Create account
				</Button>
			</span>
			<span>This feature runs in the TMGR cloud.</span>
		</template>
		<template v-else>
			<h1 v-if="module" class="text-lg font-semibold text-ink">
				{{ module }}
			</h1>
			<p class="text-sm font-medium text-ink">Requires an account</p>
			<p class="text-sm text-ink-subtle">This feature runs in the TMGR cloud.</p>
			<Button @click="createAccount">Create account</Button>
		</template>
	</div>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import { defineComponent } from 'vue';
	import { useRouter } from 'vue-router';

	export default defineComponent({
		name: 'RequiresAccount',
		components: { Button },
		props: {
			module: { type: String, default: '' },
			compact: { type: Boolean, default: false },
		},
		setup() {
			const router = useRouter();
			const createAccount = () => router.push({ name: 'Login' });
			return { createAccount };
		},
	});
</script>
