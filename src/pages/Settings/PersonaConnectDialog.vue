<template>
	<Dialog :open="open" @update:open="onOpenChange">
		<DialogContent class="sm:max-w-[480px]">
			<DialogHeader>
				<DialogTitle>Connect an agent</DialogTitle>
			</DialogHeader>

			<template v-if="!issuedSecret">
				<p v-if="loadingWorkspaces" class="text-sm text-muted-foreground">
					Loading workspaces…
				</p>
				<p
					v-else-if="eligibleWorkspaces.length === 0"
					class="text-sm text-muted-foreground"
				>
					This persona has no active workspace grant yet. Grant it access
					under Workspaces above, then come back here.
				</p>
				<form v-else class="flex flex-col gap-3" @submit.prevent="submit">
					<label class="flex flex-col gap-1">
						<span class="text-sm font-medium">Workspace</span>
						<select
							v-model.number="form.workspace_id"
							class="rounded-md border border-border bg-background px-2 py-1.5 text-sm"
						>
							<option
								v-for="ws in eligibleWorkspaces"
								:key="ws.id"
								:value="ws.id"
							>
								{{ ws.name }}
							</option>
						</select>
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
							v-model.number="form.expires_in_days"
							class="rounded-md border border-border bg-background px-2 py-1.5 text-sm"
						>
							<option v-for="days in EXPIRY_OPTIONS" :key="days" :value="days">
								{{ days }} days
							</option>
						</select>
					</label>

					<p class="text-xs text-muted-foreground">
						Unlike your device token, this token works in one workspace only,
						is limited to this persona's grant there, and everything it writes
						is signed as this persona.
					</p>

					<DialogFooter>
						<Button type="submit" :disabled="issuing">
							{{ issuing ? 'Issuing…' : 'Issue token' }}
						</Button>
					</DialogFooter>
				</form>
			</template>

			<template v-else>
				<p class="text-sm font-medium text-amber-600 dark:text-amber-400">
					Copy this token now — you won't see it again.
				</p>
				<div class="flex items-center gap-2">
					<code
						class="min-w-0 flex-1 overflow-x-auto rounded bg-muted px-2 py-1.5 font-mono text-xs"
					>
						{{ issuedSecret }}
					</code>
					<Button type="button" variant="outline" size="sm" @click="copySecret">
						{{ copied ? 'Copied' : 'Copy' }}
					</Button>
				</div>

				<div class="flex flex-col gap-2">
					<p class="text-sm">
						Add it to your agent's <code>.mcp.json</code>:
					</p>
					<pre
						class="overflow-x-auto whitespace-pre rounded bg-muted p-3 font-mono text-xs"
						>{{ snippet }}</pre
					>
					<p class="text-xs text-muted-foreground">
						Put the token itself in an environment variable or a gitignored
						secret file — never commit it.
					</p>
				</div>

				<DialogFooter>
					<Button type="button" @click="close">Done</Button>
				</DialogFooter>
			</template>
		</DialogContent>
	</Dialog>
</template>

<script lang="ts">
	import {
		issuePersonaToken,
		listWorkspacePersonas,
		type Persona,
		type PersonaToken,
	} from '@/actions/tmgr/personas';
	import { getWorkspaces } from '@/actions/tmgr/workspaces';
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
		buildMcpUrl,
		buildPersonaTokenSnippet,
		DEFAULT_TOKEN_EXPIRY_DAYS,
		EXPIRY_OPTIONS,
		extractFieldErrors,
		validateTokenLabel,
	} from '@/utils/personas';
	import { computed, defineComponent, reactive, ref, watch, type PropType } from 'vue';

	interface EligibleWorkspace {
		id: number;
		name: string;
	}

	export default defineComponent({
		name: 'PersonaConnectDialog',
		components: { Button, Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, Input },
		props: {
			persona: { type: Object as PropType<Persona>, required: true },
			open: { type: Boolean, default: false },
		},
		emits: ['update:open', 'issued'],
		setup(props, { emit }) {
			const eligibleWorkspaces = ref<EligibleWorkspace[]>([]);
			const loadingWorkspaces = ref(false);
			const form = reactive({
				workspace_id: null as number | null,
				label: '',
				expires_in_days: DEFAULT_TOKEN_EXPIRY_DAYS,
			});
			const fieldErrors = ref<Record<string, string[]>>({});
			const issuing = ref(false);
			const issuedSecret = ref<string | null>(null);
			const issuedToken = ref<PersonaToken | null>(null);
			const copied = ref(false);

			const reset = () => {
				eligibleWorkspaces.value = [];
				form.workspace_id = null;
				form.label = '';
				form.expires_in_days = DEFAULT_TOKEN_EXPIRY_DAYS;
				fieldErrors.value = {};
				issuing.value = false;
				issuedSecret.value = null;
				issuedToken.value = null;
				copied.value = false;
			};

			const loadEligibleWorkspaces = async () => {
				loadingWorkspaces.value = true;
				try {
					const workspaces = (await getWorkspaces()).filter(
						(w) => !w.is_local,
					);
					const results = await Promise.all(
						workspaces.map(async (ws) => {
							try {
								const grants = await listWorkspacePersonas(ws.id);
								const grant = grants.find(
									(g) => g.persona.id === props.persona.id,
								);
								return grant && !grant.blocked
									? { id: ws.id, name: ws.name }
									: null;
							} catch {
								return null;
							}
						}),
					);
					eligibleWorkspaces.value = results.filter(
						(w): w is EligibleWorkspace => !!w,
					);
					if (eligibleWorkspaces.value.length > 0) {
						form.workspace_id = eligibleWorkspaces.value[0].id;
					}
				} finally {
					loadingWorkspaces.value = false;
				}
			};

			watch(
				() => props.open,
				(isOpen) => {
					if (isOpen) {
						loadEligibleWorkspaces();
					} else {
						reset();
					}
				},
			);

			const onOpenChange = (value: boolean) => emit('update:open', value);
			const close = () => emit('update:open', false);

			const submit = async () => {
				if (!form.workspace_id) return;
				const labelError = validateTokenLabel(form.label);
				if (labelError) {
					fieldErrors.value = { label: [labelError] };
					return;
				}
				issuing.value = true;
				fieldErrors.value = {};
				try {
					const result = await issuePersonaToken(props.persona.id, {
						workspace_id: form.workspace_id,
						label: form.label.trim(),
						expires_in_days: form.expires_in_days,
					});
					issuedSecret.value = result.secret;
					issuedToken.value = result.token;
					emit('issued');
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

			const copySecret = async () => {
				if (!issuedSecret.value) return;
				try {
					await navigator.clipboard.writeText(issuedSecret.value);
					copied.value = true;
					setTimeout(() => (copied.value = false), 2000);
				} catch {
					toast({ title: 'Could not copy to clipboard', variant: 'destructive' });
				}
			};

			const mcpUrl = computed(() =>
				buildMcpUrl((import.meta as any).env.VITE_API_BASE_URL),
			);
			const snippet = computed(() => buildPersonaTokenSnippet(mcpUrl.value));

			return {
				eligibleWorkspaces,
				loadingWorkspaces,
				form,
				fieldErrors,
				issuing,
				issuedSecret,
				copied,
				onOpenChange,
				close,
				submit,
				copySecret,
				snippet,
				EXPIRY_OPTIONS,
			};
		},
	});
</script>
