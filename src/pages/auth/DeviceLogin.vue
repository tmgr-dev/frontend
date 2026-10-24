<template>
	<PageContainer width="full">
		<div class="mx-auto w-full max-w-[480px] px-4 pt-6 pb-10 md:pt-12">
			<div
				class="rounded-lg border border-border bg-card p-4 text-card-foreground sm:p-6"
			>
				<h1 class="mb-4 text-lg font-semibold text-ink">{{ title }}</h1>

				<div class="space-y-4 text-sm text-muted-foreground" role="status">
					<template v-if="approval.state.value === 'entry'">
						<p>Enter the code shown in your terminal.</p>
						<form class="space-y-3" @submit.prevent="approval.find()">
							<Label for="device-code">Code</Label>
							<Input
								id="device-code"
								:model-value="approval.code.value"
								class="h-11 font-mono text-lg tracking-widest uppercase"
								placeholder="XXXX-XXXX"
								autocomplete="off"
								autocapitalize="characters"
								spellcheck="false"
								autofocus
								@update:model-value="approval.setCode"
							/>
							<Button
								type="submit"
								class="h-10 w-full"
								:disabled="!approval.canLookup.value"
							>
								Continue
							</Button>
						</form>
					</template>

					<p v-else-if="approval.state.value === 'loading'">Looking up code…</p>

					<template
						v-else-if="
							approval.state.value === 'ready' ||
							approval.state.value === 'submitting'
						"
					>
						<dl class="space-y-1 text-foreground">
							<div class="flex gap-2">
								<dt class="w-20 shrink-0 text-muted-foreground">Client</dt>
								<dd class="min-w-0 break-words" data-testid="device-client">
									{{ approval.request.value.client_name }}
								</dd>
							</div>
							<div class="flex gap-2">
								<dt class="w-20 shrink-0 text-muted-foreground">Device</dt>
								<dd class="min-w-0 break-words" data-testid="device-host">
									{{ approval.request.value.host_label }}
								</dd>
							</div>
							<div class="flex gap-2">
								<dt class="w-20 shrink-0 text-muted-foreground">Requested</dt>
								<dd>{{ requestedAt }}</dd>
							</div>
							<div class="flex gap-2">
								<dt class="w-20 shrink-0 text-muted-foreground">Code</dt>
								<dd class="font-mono">{{ approval.code.value }}</dd>
							</div>
						</dl>

						<fieldset class="space-y-2">
							<legend class="mb-1 font-medium text-foreground">
								This device will be able to
							</legend>
							<ScopeChecklist
								:scopes="approval.request.value.requested_scopes"
								:selected="approval.selected.value"
								:disabled="approval.state.value === 'submitting'"
								@toggle="approval.toggle"
							/>
							<p
								v-if="approval.selected.value.length === 0"
								class="text-xs text-destructive"
							>
								Select at least one permission to approve.
							</p>
						</fieldset>

						<p
							class="rounded-md border border-border bg-muted px-3 py-2 text-xs text-foreground"
						>
							Only approve if you started <code>tmgr login</code> yourself on
							that device.
						</p>

						<div class="flex gap-2">
							<Button
								class="h-10 flex-1"
								:disabled="
									!approval.canApprove.value ||
									approval.state.value === 'submitting'
								"
								@click="approval.approve()"
							>
								Approve
							</Button>
							<Button
								variant="outline"
								class="h-10 flex-1"
								:disabled="approval.state.value === 'submitting'"
								@click="approval.deny()"
							>
								Deny
							</Button>
						</div>
					</template>

					<p v-else-if="approval.state.value === 'approved'">
						Device approved. You can return to your terminal.
					</p>
					<p v-else-if="approval.state.value === 'denied'">
						Request denied. Nothing was signed in.
					</p>

					<template v-else>
						<p v-if="approval.state.value === 'not_found'">
							This code is invalid or has expired. Run
							<CommandChip command="tmgr login" /> again.
						</p>
						<p v-else-if="approval.state.value === 'locked'">
							{{ lockedMessage }}
						</p>
						<p v-else>Something went wrong. Please try again in a moment.</p>
						<Button
							variant="outline"
							class="h-10 w-full"
							@click="
								approval.state.value === 'error'
									? approval.retry()
									: approval.reset()
							"
						>
							{{
								approval.state.value === 'error'
									? 'Try again'
									: 'Enter another code'
							}}
						</Button>
					</template>

					<router-link
						v-if="finished"
						to="/app"
						class="block text-center font-medium text-primary underline underline-offset-4"
						>Go to TMGR</router-link
					>
				</div>
			</div>
		</div>
	</PageContainer>
</template>

<script>
	import {
		approveDeviceRequest,
		denyDeviceRequest,
		getDeviceRequest,
	} from '@/actions/tmgr/devices';
	import ScopeChecklist from '@/components/devices/ScopeChecklist.vue';
	import CommandChip from '@/components/general/CommandChip.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { createDeviceApproval } from '@/composable/useDeviceApproval';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { computed, defineComponent, onMounted } from 'vue';
	import { useRoute } from 'vue-router';

	const TITLES = {
		entry: 'Approve a device',
		loading: 'Approve a device',
		ready: 'Approve this device?',
		submitting: 'Approve this device?',
		approved: 'Device approved',
		denied: 'Request denied',
		not_found: 'Invalid code',
		locked: 'Too many attempts',
		error: 'Something went wrong',
	};

	export default defineComponent({
		name: 'DeviceLogin',
		components: {
			PageContainer,
			Button,
			CommandChip,
			Input,
			Label,
			ScopeChecklist,
		},
		setup() {
			const route = useRoute();
			const approval = createDeviceApproval({
				lookup: getDeviceRequest,
				approve: approveDeviceRequest,
				deny: denyDeviceRequest,
			});

			const title = computed(() => TITLES[approval.state.value]);
			const finished = computed(() =>
				['approved', 'denied'].includes(approval.state.value),
			);
			const requestedAt = computed(() => {
				const value = approval.request.value?.created_at;
				const date = value ? new Date(value) : null;
				return date && !Number.isNaN(date.getTime())
					? date.toLocaleString()
					: '';
			});

			onMounted(() => {
				setDocumentTitle('Approve a device');
				const raw = route.query.code;
				approval.setCode(Array.isArray(raw) ? raw[0] : raw);
				if (approval.canLookup.value) approval.find();
			});

			const lockedMessage = computed(() => {
				const seconds = approval.retryAfter.value;
				if (!seconds)
					return 'Too many wrong codes. Try again in a few minutes.';
				const minutes = Math.max(1, Math.ceil(seconds / 60));
				return `Too many wrong codes. Try again in ${minutes} minute${
					minutes === 1 ? '' : 's'
				}.`;
			});

			return { approval, title, finished, requestedAt, lockedMessage };
		},
	});
</script>
