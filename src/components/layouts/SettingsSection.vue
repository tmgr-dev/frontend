<template>
	<section
		:class="[
			'rounded-lg border bg-card p-4 text-card-foreground sm:p-6',
			tone === 'danger'
				? 'border-destructive/40 dark:border-destructive/50'
				: 'border-border',
		]"
		data-settings-section
	>
		<header
			v-if="title || description || $slots.description || $slots.actions"
			:class="[
				'flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between',
				$slots.default && 'mb-4',
			]"
		>
			<div class="min-w-0">
				<h2
					v-if="title"
					:class="[
						'text-base font-semibold',
						tone === 'danger' ? 'text-destructive' : 'text-ink',
					]"
				>
					{{ title }}
				</h2>
				<p
					v-if="description || $slots.description"
					class="mt-1 text-sm text-ink-subtle"
				>
					<slot name="description">{{ description }}</slot>
				</p>
			</div>
			<div
				v-if="$slots.actions"
				class="flex flex-wrap items-center gap-2 sm:shrink-0"
			>
				<slot name="actions" />
			</div>
		</header>
		<slot />
		<footer
			v-if="$slots.footer"
			class="mt-6 flex flex-wrap items-center justify-end gap-2 border-t border-border pt-4"
		>
			<slot name="footer" />
		</footer>
	</section>
</template>

<script>
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'SettingsSection',
		props: {
			title: { type: String, default: '' },
			description: { type: String, default: '' },
			tone: {
				type: String,
				default: 'default',
				validator: (value) => ['default', 'danger'].includes(value),
			},
		},
	});
</script>
