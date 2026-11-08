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
				<span class="font-medium text-ink">{{
					$t('general.requiresAccount.title')
				}}</span>
				<Button variant="outline" size="sm" @click="createAccount">
					{{ $t('general.requiresAccount.createAccount') }}
				</Button>
			</span>
			<span>{{ $t('general.requiresAccount.cloudOnly') }}</span>
		</template>
		<template v-else>
			<h1 v-if="module" class="text-lg font-semibold text-ink">
				{{ module }}
			</h1>
			<p class="text-sm font-medium text-ink">
				{{ $t('general.requiresAccount.title') }}
			</p>
			<p class="text-sm text-ink-subtle">
				{{ $t('general.requiresAccount.cloudOnly') }}
			</p>
			<Button @click="createAccount">{{
				$t('general.requiresAccount.createAccount')
			}}</Button>
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
			const createAccount = () => router.push({ name: 'Register' });
			return { createAccount };
		},
	});
</script>
