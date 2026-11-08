<template>
	<div>
		<div :class="rowClass">
			<pre
				:aria-label="labelText"
				:class="[
					textClass,
					'rounded-md border border-line bg-surface-sunken px-3 py-3 font-mono text-xs text-ink',
				]"
				data-testid="snippet-text"
				>{{ display ?? code }}</pre>
			<button
				type="button"
				:class="[
					buttonClass,
					'inline-flex min-h-[44px] items-center rounded-md border border-line bg-surface px-3 text-sm font-medium text-ink hover:bg-surface-sunken sm:min-h-[32px]',
				]"
				data-testid="snippet-copy"
				:disabled="!code"
				:aria-label="
					$t('onboarding.copyCommand.copyLabel', { label: labelText })
				"
				@click="copy"
			>
				{{ copied ? $t('common.actions.copied') : $t('common.actions.copy') }}
			</button>
			<span class="sr-only" role="status" aria-live="polite">{{
				copied ? $t('common.actions.copied') : ''
			}}</span>
		</div>
		<p v-if="failed" class="mt-1 text-xs text-status-fix-fg" role="alert">
			{{ $t('onboarding.copyCommand.failed') }}
		</p>
	</div>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import { tourBus } from '@/tours/events';
	import {
		SNIPPET_BUTTON_CLASS,
		SNIPPET_ROW_CLASS,
		snippetTextClass,
	} from '@/utils/snippetLayout';
	import { computed, defineComponent, onBeforeUnmount, ref } from 'vue';

	export default defineComponent({
		name: 'CopyCommand',
		props: {
			code: { type: String, required: true },
			display: { type: String, default: undefined },
			label: { type: String, default: '' },
			client: { type: String, default: '' },
			emitEvent: { type: Boolean, default: true },
			wrap: { type: Boolean, default: false },
		},
		emits: ['copied'],
		setup(props, { emit }) {
			const copied = ref(false);
			const failed = ref(false);
			let timer: ReturnType<typeof setTimeout> | null = null;

			const copy = async () => {
				failed.value = false;
				try {
					await navigator.clipboard.writeText(props.code);
				} catch {
					failed.value = true;
					return;
				}
				copied.value = true;
				if (timer) clearTimeout(timer);
				timer = setTimeout(() => (copied.value = false), 2000);
				if (props.emitEvent)
					tourBus.emit('connect.command_copied', { client: props.client });
				emit('copied');
			};

			onBeforeUnmount(() => {
				if (timer) clearTimeout(timer);
			});

			return {
				copied,
				failed,
				copy,
				labelText: computed(
					() => props.label || t('onboarding.copyCommand.defaultLabel'),
				),
				rowClass: SNIPPET_ROW_CLASS,
				buttonClass: SNIPPET_BUTTON_CLASS,
				textClass: computed(() => snippetTextClass(props.wrap)),
			};
		},
	});
</script>
