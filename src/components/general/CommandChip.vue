<template>
	<span
		class="inline-flex max-w-full items-center gap-0.5 rounded bg-muted py-0.5 pr-0.5 pl-1.5 align-middle font-mono text-xs text-ink"
		data-testid="command-chip"
	>
		<code class="break-all">{{ command }}</code>
		<button
			type="button"
			class="inline-flex size-5 shrink-0 items-center justify-center rounded text-ink-muted hover:bg-surface-sunken hover:text-ink"
			:aria-label="$t('general.commandChip.copyCommand', { command })"
			:title="$t('common.actions.copy')"
			data-testid="command-chip-copy"
			@click="copyCommand"
		>
			<Copy class="size-3.5" aria-hidden="true" />
		</button>
	</span>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import { toast } from '@/components/ui/toast';
	import { useCopyToClipboard } from '@/composable/useCopyToClipboard';
	import { Copy } from 'lucide-vue-next';
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'CommandChip',
		components: { Copy },
		props: {
			command: { type: String, required: true },
		},
		setup(props) {
			const [copy] = useCopyToClipboard();

			const copyCommand = async () => {
				const ok = await copy(props.command);
				toast(
					ok
						? { title: t('common.actions.copied'), description: props.command }
						: {
								title: t('general.commandChip.copyFailed'),
								variant: 'destructive',
							},
				);
			};

			return { copyCommand };
		},
	});
</script>
