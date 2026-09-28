<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent class="sm:max-w-[480px]">
			<DialogHeader>
				<DialogTitle>Connect an agent</DialogTitle>
			</DialogHeader>

			<template v-if="!issuedToken">
				<form class="flex flex-col gap-3" @submit.prevent="submit">
					<label class="flex flex-col gap-1">
						<span class="text-sm font-medium">Workspace</span>
						<p class="text-sm text-muted-foreground">{{ workspace.name }}</p>
					</label>

					<label class="flex flex-col gap-1">
						<span class="text-sm font-medium">Label</span>
						<Input
							:model-value="form.label"
							maxlength="60"
							placeholder="Claude Code on my laptop"
							@update:model-value="(v) => (form.label = String(v))"
						/>
						<span
							v-if="fieldErrors.label"
							class="text-xs text-red-600 dark:text-red-400"
						>
							{{ fieldErrors.label.join(' ') }}
						</span>
					</label>

					<label class="flex flex-col gap-1">
						<span class="text-sm font-medium">Expires</span>
						<select
							v-model.number="form.expiresInDays"
							class="rounded-md border border-border bg-background px-2 py-1.5 text-sm"
						>
							<option v-for="days in EXPIRY_OPTIONS" :key="days" :value="days">
								{{ days }} days
							</option>
						</select>
					</label>

					<p class="text-xs text-muted-foreground">
						{{ BOUNDARY_LINE }}
					</p>

					<DialogFooter>
						<Button type="submit" :disabled="issuing">
							{{ issuing ? 'Issuing…' : 'Issue token' }}
						</Button>
					</DialogFooter>
				</form>
			</template>

			<template v-else>
				<p class="text-sm font-medium text-emerald-600 dark:text-emerald-400">
					Token issued.
				</p>
				<p class="text-sm">
					<code class="rounded bg-muted px-2 py-1 font-mono text-xs">{{ issuedToken.prefix }}…</code>
					expires {{ formatDate(issuedToken.expiresAt) }}
				</p>

				<Button type="button" variant="outline" size="sm" class="self-start" @click="copyToken">
					{{ copiedToken ? 'Copied' : 'Copy token' }}
				</Button>

				<div class="flex flex-col gap-2">
					<p class="text-sm font-medium">MCP config</p>

					<p class="text-xs text-muted-foreground">Codex (<code>~/.codex/config.toml</code>)</p>
					<pre
						class="overflow-x-auto whitespace-pre rounded bg-muted p-3 font-mono text-xs"
						>{{ codexSnippet }}</pre
					>

					<p class="text-xs text-muted-foreground">Claude Code</p>
					<pre
						class="overflow-x-auto whitespace-pre rounded bg-muted p-3 font-mono text-xs"
						>{{ claudeSnippet }}</pre
					>

					<p class="text-xs text-muted-foreground">
						Other companions: read the token id from
						<code>--token-id {{ issuedToken.id }}</code>, or point
						<code>TMGR_LOCAL_TOKEN</code> at the secret.
					</p>

					<Button type="button" variant="outline" size="sm" class="self-start" @click="copyConfig">
						{{ copiedConfig ? 'Copied' : 'Copy MCP config' }}
					</Button>
				</div>

				<DialogFooter>
					<Button type="button" @click="close">Done</Button>
				</DialogFooter>
			</template>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Input } from '@/components/ui/input';
	import { toast } from '@/components/ui/toast';
	import {
		copyLocalToken,
		getLocalAccessStatus,
		issueLocalToken,
		type TokenInfo,
	} from '@/local/localTokens';
	import type { WorkspacePersonaRow } from '@/local/personas';
	import type { LocalWorkspace } from '@/local/types';
	import {
		DEFAULT_TOKEN_EXPIRY_DAYS,
		EXPIRY_OPTIONS,
		extractFieldErrors,
		validateTokenLabel,
	} from '@/utils/personas';
	import { computed, defineComponent, reactive, ref, watch, type PropType } from 'vue';

	const BOUNDARY_LINE =
		"The token keeps an agent inside this persona's permissions and signs everything it writes. It is not a lock against other programs on this Mac: anything running as you can read this workspace's file directly.";

	export default defineComponent({
		name: 'LocalPersonaConnectDialog',
		components: { Button, Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, Input },
		props: {
			persona: { type: Object as PropType<WorkspacePersonaRow>, required: true },
			workspace: { type: Object as PropType<LocalWorkspace>, required: true },
			open: { type: Boolean, default: false },
			initialLabel: { type: String, default: '' },
			pluginId: { type: String as PropType<string | null>, default: null },
		},
		emits: ['update:open', 'issued', 'cancelled'],
		setup(props, { emit }) {
			const form = reactive({
				label: props.initialLabel,
				expiresInDays: DEFAULT_TOKEN_EXPIRY_DAYS as number,
			});
			const fieldErrors = ref<Record<string, string[]>>({});
			const issuing = ref(false);
			const issuedToken = ref<TokenInfo | null>(null);
			const bridgeCommand = ref('');
			const copiedToken = ref(false);
			const copiedConfig = ref(false);

			const reset = () => {
				form.label = props.initialLabel;
				form.expiresInDays = DEFAULT_TOKEN_EXPIRY_DAYS;
				fieldErrors.value = {};
				issuing.value = false;
				issuedToken.value = null;
				bridgeCommand.value = '';
				copiedToken.value = false;
				copiedConfig.value = false;
			};

			watch(
				() => props.open,
				(isOpen) => {
					if (!isOpen) reset();
				},
			);

			const onOpenChange = (value: boolean) => {
				if (!value && !issuedToken.value) emit('cancelled');
				emit('update:open', value);
			};
			const close = () => emit('update:open', false);

			const submit = async () => {
				const labelError = validateTokenLabel(form.label);
				if (labelError) {
					fieldErrors.value = { label: [labelError] };
					return;
				}
				issuing.value = true;
				fieldErrors.value = {};
				try {
					const [token, status] = await Promise.all([
						issueLocalToken({
							personaUuid: props.persona.uuid,
							personaName: props.persona.name,
							workspaceCode: props.workspace.code,
							label: form.label.trim(),
							expiresInDays: form.expiresInDays,
							...(props.pluginId ? { pluginId: props.pluginId } : {}),
						}),
						getLocalAccessStatus(),
					]);
					issuedToken.value = token;
					bridgeCommand.value = status.bridgeCommand;
					emit('issued', token);
				} catch (error) {
					const errors = extractFieldErrors(error);
					if (errors) {
						fieldErrors.value = errors.errors;
					} else {
						toast({ title: 'Could not issue token', variant: 'destructive' });
					}
				} finally {
					issuing.value = false;
				}
			};

			const codexSnippet = computed(() => {
				if (!issuedToken.value) return '';
				return [
					'[mcp_servers.tmgr-local]',
					`command = "${bridgeCommand.value}"`,
					`args = ["mcp", "--token-id", "${issuedToken.value.id}"]`,
				].join('\n');
			});

			const claudeSnippet = computed(() => {
				if (!issuedToken.value) return '';
				return `claude mcp add tmgr-local -- "${bridgeCommand.value}" mcp --token-id ${issuedToken.value.id}`;
			});

			const mcpConfigText = computed(() => {
				if (!issuedToken.value) return '';
				return [
					'# Codex (~/.codex/config.toml)',
					codexSnippet.value,
					'',
					'# Claude Code',
					claudeSnippet.value,
					'',
					'# Other companions',
					`Read the token id from --token-id ${issuedToken.value.id}, or point TMGR_LOCAL_TOKEN at the secret.`,
				].join('\n');
			});

			const copyToken = async () => {
				if (!issuedToken.value) return;
				try {
					await copyLocalToken(issuedToken.value.id);
					copiedToken.value = true;
					toast({ title: 'Copied to clipboard' });
					setTimeout(() => (copiedToken.value = false), 2000);
				} catch {
					toast({ title: 'Could not copy token', variant: 'destructive' });
				}
			};

			const copyConfig = async () => {
				try {
					await navigator.clipboard.writeText(mcpConfigText.value);
					copiedConfig.value = true;
					toast({ title: 'Copied to clipboard' });
					setTimeout(() => (copiedConfig.value = false), 2000);
				} catch {
					toast({ title: 'Could not copy to clipboard', variant: 'destructive' });
				}
			};

			const formatDate = (date: string) => new Date(date).toLocaleDateString();

			return {
				form,
				fieldErrors,
				issuing,
				issuedToken,
				copiedToken,
				copiedConfig,
				onOpenChange,
				close,
				submit,
				codexSnippet,
				claudeSnippet,
				copyToken,
				copyConfig,
				formatDate,
				EXPIRY_OPTIONS,
				BOUNDARY_LINE,
			};
		},
	});
</script>
