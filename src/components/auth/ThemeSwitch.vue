<template>
	<div
		class="inline-flex items-center gap-0.5 rounded-lg border border-border bg-card p-0.5 shadow-xs"
		role="radiogroup"
		:aria-label="$t('auth.theme.label')"
	>
		<button
			v-for="option in options"
			:key="option.value"
			type="button"
			role="radio"
			:aria-checked="current === option.value"
			:title="option.title"
			:class="[
				'flex h-7 w-7 items-center justify-center rounded-md transition-colors',
				current === option.value
					? 'bg-muted text-foreground'
					: 'text-muted-foreground hover:text-foreground',
			]"
			@click="select(option.value)"
		>
			<component :is="option.icon" class="h-3.5 w-3.5" />
		</button>
	</div>
</template>

<script>
	import { t } from '@/i18n';
	import store from '@/store';
	import { Monitor, Moon, Sun } from 'lucide-vue-next';
	import { computed, defineComponent } from 'vue';

	export default defineComponent({
		name: 'ThemeSwitch',
		setup() {
			const options = computed(() => [
				{ value: 'system', title: t('auth.theme.system'), icon: Monitor },
				{ value: 'default', title: t('auth.theme.light'), icon: Sun },
				{ value: 'dark', title: t('auth.theme.dark'), icon: Moon },
			]);
			const current = computed(() => store.state.colorScheme);
			const select = (value) => store.commit('setColorScheme', value);

			return { options, current, select };
		},
	});
</script>
