<template>
	<article
		class="rounded-lg border border-border bg-card p-4 text-card-foreground"
		data-testid="machine-limit-card"
	>
		<header class="flex items-start justify-between gap-2">
			<div class="min-w-0">
				<h3
					class="truncate text-sm font-semibold text-ink"
					:title="machine.label"
				>
					{{ machine.label }}
				</h3>
				<p class="text-xs text-ink-subtle">
					{{ platform }} ·
					{{
						$t('agents.usage.machines.limits.seen', {
							age: ageText(machine.last_seen_at, now),
						})
					}}
				</p>
			</div>
			<DropdownMenu>
				<DropdownMenuTrigger as-child>
					<Button
						variant="ghost"
						size="icon"
						class="size-8 shrink-0"
						:aria-label="$t('agents.usage.machines.remove.menu')"
						data-testid="machine-menu"
					>
						<MoreHorizontal aria-hidden="true" />
					</Button>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="end">
					<DropdownMenuItem
						class="text-destructive"
						data-testid="machine-remove"
						@select="confirmOpen = true"
					>
						{{ $t('agents.usage.machines.remove.action') }}
					</DropdownMenuItem>
				</DropdownMenuContent>
			</DropdownMenu>
		</header>

		<p v-if="!rows.length" class="mt-4 text-sm text-ink-subtle">
			{{ $t('agents.usage.machines.limits.none') }}
		</p>
		<ul v-else class="mt-4 flex flex-col gap-4">
			<li v-for="r in rows" :key="r.key" :data-level="r.level">
				<div class="flex flex-wrap items-baseline justify-between gap-x-3">
					<span class="min-w-0 text-sm font-medium text-ink">
						{{ r.label }}
						<span v-if="showAccount" class="font-normal text-ink-subtle">
							· {{ r.account }}
						</span>
					</span>
					<span class="text-sm text-ink tabular-nums" data-testid="limit-pct">
						{{
							r.reset
								? $t('agents.usage.machines.limits.reset')
								: `${Math.round(r.pct)}%`
						}}
					</span>
				</div>
				<div
					class="mt-1 h-2 overflow-hidden rounded-full bg-muted"
					role="progressbar"
					:aria-valuenow="Math.round(r.pct)"
					aria-valuemin="0"
					aria-valuemax="100"
					:aria-label="r.label"
				>
					<div
						class="h-full rounded-full"
						:class="TONES[r.level]"
						:style="{ width: r.pct + '%' }"
					/>
				</div>
				<p class="mt-1 text-xs text-ink-subtle">
					<span v-if="r.countdown" data-testid="limit-countdown">{{
						$t('agents.usage.machines.limits.resetsIn', { time: r.countdown })
					}}</span>
					<span v-if="r.countdown"> · </span>
					<span>{{ r.asOf }}</span>
				</p>
			</li>
		</ul>

		<AlertDialog v-model:open="confirmOpen">
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>{{
						$t('agents.usage.machines.remove.title', { label: machine.label })
					}}</AlertDialogTitle>
					<AlertDialogDescription>
						{{
							$t('agents.usage.machines.remove.body', {
								label: machine.label,
								command: 'tmgr usage machine on',
							})
						}}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>{{
						$t('agents.usage.machines.remove.cancel')
					}}</AlertDialogCancel>
					<Button
						type="button"
						variant="destructive"
						data-testid="machine-remove-confirm"
						@click="confirm"
					>
						{{ $t('agents.usage.machines.remove.confirm') }}
					</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	</article>
</template>

<script lang="ts">
	import type { Machine } from '@/actions/tmgr/aiUsageMachines';
	import {
		AlertDialog,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { t, te } from '@/i18n';
	import { ageText, limitRows } from '@/usage/machines';
	import { MoreHorizontal } from 'lucide-vue-next';
	import { computed, defineComponent, ref, type PropType } from 'vue';

	const TONES = {
		ok: 'bg-ink-subtle',
		warn: 'bg-amber-500 dark:bg-amber-400',
		critical: 'bg-destructive',
	};

	export default defineComponent({
		name: 'MachineLimitCard',
		components: {
			AlertDialog,
			AlertDialogCancel,
			AlertDialogContent,
			AlertDialogDescription,
			AlertDialogFooter,
			AlertDialogHeader,
			AlertDialogTitle,
			Button,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuTrigger,
			MoreHorizontal,
		},
		props: {
			machine: { type: Object as PropType<Machine>, required: true },
			now: { type: Date as PropType<Date>, required: true },
		},
		emits: ['remove'],
		setup(props, { emit }) {
			const confirmOpen = ref(false);
			const rows = computed(() => limitRows(props.machine.limits, props.now));
			const showAccount = computed(
				() => new Set(rows.value.map((r) => r.account)).size > 1,
			);
			const platform = computed(() => {
				const key = `agents.usage.machines.platforms.${props.machine.platform}`;
				return te(key) ? t(key) : props.machine.platform;
			});
			return {
				TONES,
				ageText,
				confirmOpen,
				rows,
				showAccount,
				platform,
				confirm: () => {
					confirmOpen.value = false;
					emit('remove', props.machine.machine_id);
				},
			};
		},
	});
</script>
