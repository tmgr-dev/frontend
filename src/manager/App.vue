<template>
	<div class="manager app-shell" :style="shellStyle">
		<aside class="side">
			<div class="top" data-tauri-drag-region>
				<b data-tauri-drag-region>{{ $t('manager.list.label') }}</b>
				<button
					type="button"
					class="btn pri sm"
					@click="store.openNewSession()"
				>
					{{ $t('manager.list.newSession') }}
				</button>
				<button
					type="button"
					class="ibd"
					:aria-label="$t('manager.trust.devices.open')"
					:title="$t('manager.trust.devices.open')"
					@click="trust.openPanel()"
				>
					<Icon name="gear" />
				</button>
			</div>
			<button type="button" class="sidesrch" @click="store.openSearch()">
				<span>{{ $t('manager.search.scopeAll') }}</span>
				<kbd>&#8984;K</kbd>
			</button>
			<p v-if="sharedWithPhone" class="mgr-share">
				{{ $t('manager.list.sharedWithPhone') }}
			</p>
			<ConnectFollowers
				:followers="followers.followers.length"
				:pending="followers.pendingCount(pairing.machines)"
				:busy="followers.busy"
				:hints="followers.scopeHints"
				:results="followers.results"
				@connect="followers.connect()"
			/>
			<RemotePolicy
				:leaders="remotePolicy.leaders"
				:machines="pairing.machines"
				:busy="remotePolicy.busy"
				:error="remotePolicy.error"
				@toggle="remotePolicy.setAsk"
			/>
			<ChatList
				:rooms="store.rooms"
				:machines="pairing.machines"
				:active-key="store.activeKey ?? undefined"
				:unread="store.unreadByRoom"
				@select="store.select"
			/>
		</aside>
		<section class="main">
			<GateBlockersBanner :labels="gateBlockers" />
			<RunnerAddedNotice
				v-if="pairing.notice"
				:label="pairing.notice.label"
				@revoke="revokeAddedRunner"
				@dismiss="pairing.dismiss()"
			/>
			<ChatHeader
				v-if="activeInfo"
				:info="activeInfo"
				:sub="subtitle"
				@search="store.openSearch(store.activeKey ?? undefined)"
				:busy="store.control.busy"
				@switch="store.openSwitch()"
				@stop="store.stopActive()"
				@force="store.openControl('force')"
				@delete="store.openControl('delete')"
			/>
			<p
				v-if="
					!store.control.open &&
					(store.control.error || store.control.stillRunning)
				"
				class="mgr-error"
				role="alert"
			>
				{{ store.control.error || $t('manager.control.stillRunning') }}
			</p>
			<SearchPanel
				v-if="store.search.open"
				:query="store.search.text"
				:scope="searchScope"
				:chat-label="activeInfo?.title ?? ''"
				:can-manager="!!managerKey && managerKey !== store.activeKey"
				:hits="store.search.hits"
				:room-titles="roomTitles"
				@update:query="store.runSearch"
				@update:scope="onScope"
				@select="onHit"
			/>
			<template v-else>
				<MessageList
					ref="list"
					:messages="shownMessages"
					:body-html="bodyHtml"
					:empty-hint="
						activeInfo?.room.kind === 'session'
							? $t('manager.messages.emptySession')
							: undefined
					"
					:queued-ids="store.queuedIds"
					:queued-why="store.queuedWhy"
					:delivery-by-id="store.delivery"
					:delivery-who="deliveryWho"
					:delivery-machine="deliveryMachine"
					@retry="store.retry"
					:tools-by-message="store.toolsByMessage"
					:calls-by-message="store.callsByMessage"
					:activity="
						store.activeKey ? store.activityByRoom[store.activeKey] : null
					"
					@reply="store.setReply"
					@copy="copy"
					:room-key="store.activeKey ?? ''"
					:shown-rooms="store.shownRooms"
					:load-older="store.loadOlder"
					@cancel-queued="store.cancelQueued"
					:origin="activeInfo"
					:mention-supported="store.mention.supported"
					:mention-busy="store.mention.injecting"
					:mention-done="store.mention.injected"
					@open-room="openRoom"
					@inject="store.requestInject"
				>
					<template #decision="{ message }">
						<template v-if="message.poll">
							<PollCard
								v-if="pollGroupByMessage[message.id]"
								:poll="pollGroupByMessage[message.id].card.poll"
								:closed="pollGroupByMessage[message.id].closed"
								:room="message.room"
							/>
						</template>
						<template v-else-if="message.decision?.card_id">
							<template v-if="message.decision.permit">
								<PermitCard
									v-if="permitGroupByMessage[message.id]"
									:card-id="message.decision.card_id"
									:permit="permitGroupByMessage[message.id].decision.permit"
									:closed="permitGroupByMessage[message.id].closed"
									:answerable="activeInfo?.local === false"
									:live="
										permitGroupByMessage[message.id].decision.live === true
									"
									:self-fp="trust.trust?.self_fp ?? ''"
								/>
							</template>
							<template v-else-if="message.decision.closed"></template>
							<template v-else>
								<template v-if="activeInfo?.local !== false">
									<SpawnApprovalCard
										v-if="message.decision.spawn"
										:card-id="message.decision.card_id"
										:spawn="message.decision.spawn"
									/>
									<DecisionCard v-else :card-id="message.decision.card_id" />
								</template>
								<p v-else class="mgr-remote-decision">
									{{ $t('manager.decision.otherMachine') }}
								</p>
							</template>
						</template>
					</template>
				</MessageList>
				<Composer
					v-model="store.draft.text"
					:reply-to="store.replyTo ?? undefined"
					:attachments="store.draft.attachments"
					:placeholder="placeholder"
					:hint="hint"
					@update:model-value="store.onInput"
					@send="store.send"
					@clear-reply="store.setReply(null)"
					@remove-attachment="store.removeAttachment"
					@attach="pick"
					@drop="addFiles"
					@keydown="onKey"
				>
					<template #popover
						><SlashMenu
							v-if="store.slash.open"
							:commands="store.slash.items"
							:active-index="store.slash.index"
							@select="store.pickSlash" /><MentionMenu
							v-if="store.mention.open"
							:items="store.mention.items"
							:active-index="store.mention.index"
							@hover="store.mention.index = $event"
							@select="store.pickMention"
					/></template>
				</Composer>
			</template>
			<p v-if="store.error" class="mgr-error" role="alert">
				{{ store.error }}
			</p>
		</section>
		<DetailPanel
			:tab="store.panel.tab"
			:rooms="store.rooms"
			:machines="pairing.machines"
			:info="activeInfo"
			:active-key="store.activeKey ?? undefined"
			:messages="messages"
			:audit="store.panel.audit"
			:audit-error="store.panel.auditError"
			:trust="trust.trust"
			@tab="store.setTab"
			@select="store.selectFromHierarchy"
			@devices="trust.openPanel()"
		/>
		<StatusBar
			:scope="$t('manager.statusbar.scope')"
			:chat="activeInfo?.title ?? ''"
			:live="live"
		/>
		<div
			class="rz rz-left"
			role="separator"
			aria-orientation="vertical"
			:style="{ left: `${leftW - 3}px` }"
			@pointerdown="startResize('left', $event)"
			@dblclick="resetWidth('left')"
		></div>
		<div
			v-if="viewport > RIGHT_PANEL_HIDE_AT"
			class="rz rz-right"
			role="separator"
			aria-orientation="vertical"
			:style="{ right: `${rightW - 3}px` }"
			@pointerdown="startResize('right', $event)"
			@dblclick="resetWidth('right')"
		></div>
		<input ref="file" type="file" multiple hidden @change="onFiles" />
		<NewSessionDialog
			v-if="store.newSession.open"
			:form="store.newSession.form"
			:machines="store.machines"
			:machine-info="pairing.machines"
			:busy="store.newSession.busy"
			:error="store.newSession.error"
			@submit="store.submitNewSession()"
			@cancel="store.closeNewSession()"
		/>
		<ControlDialog
			v-else-if="store.control.open"
			:mode="store.control.open"
			:title="activeInfo?.title ?? ''"
			:busy="store.control.busy"
			:error="store.control.error"
			@confirm="
				store.control.open === 'delete'
					? store.deleteActive()
					: store.forceStopActive()
			"
			@cancel="store.closeControl()"
		/>
		<MentionConfirmDialog
			v-else-if="store.mention.confirm"
			:kind="store.mention.confirm.kind"
			@confirm="store.confirmMention()"
			@cancel="store.cancelMention()"
		/>
		<SwitchAgentDialog
			v-else-if="store.switcher.open"
			:agent="store.switcher.agent"
			:model="store.switcher.model"
			:models="store.switchModels"
			:labels="store.switchLabels"
			:current="activeInfo?.agent"
			:busy="store.switcher.busy"
			:error="store.switcher.error"
			@pick="store.pickSwitchAgent"
			@update:model="store.pickSwitchModel"
			@submit="store.submitSwitch()"
			@cancel="store.closeSwitch()"
		/>
		<JoinConfirmDialog
			v-else-if="trust.current?.type === 'join'"
			:label="trust.current.join.label"
			:kind="trust.current.join.kind"
			:fp="trust.current.join.fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.decideJoin(true)"
			@reject="trust.decideJoin(false)"
		/>
		<RevokeConfirmDialog
			v-else-if="trust.current?.type === 'revoke'"
			:revoker-label="trust.current.revoke.revoker_label"
			:revoker-fp="trust.current.revoke.revoker_fp"
			:target-label="trust.current.revoke.target_label"
			:target-fp="trust.current.revoke.target_fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.decideRevoke(true)"
			@reject="trust.decideRevoke(false)"
			@dismiss="trust.deferRevoke()"
		/>
		<RevokeConfirmDialog
			v-else-if="revokeDevice"
			:target-label="revokeDevice.label"
			:target-fp="revokeDevice.fp"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.confirmRevoke()"
			@reject="trust.cancelRevoke()"
			@dismiss="trust.cancelRevoke()"
		/>
		<ConfirmKindDialog
			v-else-if="confirmKindDevice && trust.confirmKindTarget"
			:label="confirmKindDevice.label"
			:fp="confirmKindDevice.fp"
			:kind="trust.confirmKindTarget.kind"
			:busy="trust.busy"
			:error="trust.error"
			@approve="trust.submitConfirmKind()"
			@cancel="trust.cancelConfirmKind()"
		/>
		<ResetIdentityDialog
			v-else-if="trust.resetOpen"
			v-model="trust.resetText"
			:busy="trust.busy"
			:error="trust.error"
			@confirm="trust.resetIdentity()"
			@cancel="trust.cancelReset()"
		/>
		<PairDeviceDialog
			v-else-if="trust.pair.mode"
			v-model="trust.pair.text"
			:mode="trust.pair.mode"
			:code="trust.pair.code"
			:qr="trust.pair.qr"
			:seconds-left="trust.pair.secondsLeft"
			:expired="trust.pair.expired"
			:loading="trust.pair.loading"
			:busy="trust.busy"
			:error="trust.error"
			@submit="trust.submitCode()"
			@renew="trust.openAddDevice()"
			@close="trust.closePair()"
		/>
		<DevicesPanel
			v-else-if="trust.panelOpen"
			:devices="trust.trust?.devices ?? []"
			:key-mismatch="trust.trust?.key_mismatch ?? []"
			:alerts="trust.trust?.alerts ?? []"
			:seq-gaps="trust.trust?.seq_gaps ?? []"
			:can-join="trust.trust?.can_join_with_code ?? false"
			:joining="trust.trust?.joining ?? false"
			:auto-join-error="trust.trust?.auto_join_error ?? null"
			:busy="trust.busy"
			:error="trust.error"
			@revoke="trust.revoke"
			@confirm-kind="trust.askConfirmKind"
			@reset="trust.openReset()"
			@add-device="trust.openAddDevice()"
			@enter-code="trust.openEnterCode()"
			@close="trust.closePanel()"
		/>
	</div>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';
	import { onManagerResync } from './api';
	import ChatHeader from './components/ChatHeader.vue';
	import ChatList from './components/ChatList.vue';
	import Composer from './components/Composer.vue';
	import ConnectFollowers from './components/ConnectFollowers.vue';
	import ControlDialog from './components/ControlDialog.vue';
	import DecisionCard from './components/DecisionCard.vue';
	import DetailPanel from './components/DetailPanel.vue';
	import GateBlockersBanner from './components/GateBlockersBanner.vue';
	import Icon from './components/Icon.vue';
	import MentionConfirmDialog from './components/MentionConfirmDialog.vue';
	import MentionMenu from './components/MentionMenu.vue';
	import MessageList from './components/MessageList.vue';
	import NewSessionDialog from './components/NewSessionDialog.vue';
	import PermitCard from './components/PermitCard.vue';
	import PollCard from './components/PollCard.vue';
	import RemotePolicy from './components/RemotePolicy.vue';
	import RunnerAddedNotice from './components/RunnerAddedNotice.vue';
	import SearchPanel from './components/SearchPanel.vue';
	import SlashMenu from './components/SlashMenu.vue';
	import SpawnApprovalCard from './components/SpawnApprovalCard.vue';
	import StatusBar from './components/StatusBar.vue';
	import SwitchAgentDialog from './components/SwitchAgentDialog.vue';
	import ConfirmKindDialog from './components/trust/ConfirmKindDialog.vue';
	import DevicesPanel from './components/trust/DevicesPanel.vue';
	import JoinConfirmDialog from './components/trust/JoinConfirmDialog.vue';
	import PairDeviceDialog from './components/trust/PairDeviceDialog.vue';
	import ResetIdentityDialog from './components/trust/ResetIdentityDialog.vue';
	import RevokeConfirmDialog from './components/trust/RevokeConfirmDialog.vue';
	import {
		composerPlaceholder,
		headerSubtitle,
		slashHint,
	} from './components/viewHelpers';
	import { useFollowersStore } from './followersStore';
	import { renderBody } from './format';
	import { sharedWithPhone } from './machineTree';
	import { usePairingStore } from './pairingStore';
	import {
		clampWidth,
		DEFAULT_WIDTH,
		loadWidth,
		RIGHT_PANEL_HIDE_AT,
		saveWidth,
		shellColumns,
		type PanelSide,
	} from './panelWidths';
	import { permitGroups } from './permitCards';
	import { pollGroups, pollHidden } from './pollCards';
	import { useRemotePolicyStore } from './remotePolicyStore';
	import { MAX_FILE_BYTES, useManagerStore } from './store';
	import { useTrustStore } from './trustStore';
	import { externalVoice } from './tts/externalVoice';
	import { roomKey, type RoomRef } from './types';

	const sha256Hex = async (file: File): Promise<string> => {
		const digest = await crypto.subtle.digest(
			'SHA-256',
			await file.arrayBuffer(),
		);
		return Array.from(new Uint8Array(digest), (b) =>
			b.toString(16).padStart(2, '0'),
		).join('');
	};

	export default defineComponent({
		name: 'ManagerApp',
		components: {
			ChatHeader,
			ControlDialog,
			ChatList,
			Composer,
			ConfirmKindDialog,
			DecisionCard,
			PermitCard,
			PollCard,
			SpawnApprovalCard,
			DetailPanel,
			DevicesPanel,
			ConnectFollowers,
			RemotePolicy,
			GateBlockersBanner,
			PairDeviceDialog,
			Icon,
			JoinConfirmDialog,
			MentionConfirmDialog,
			MentionMenu,
			MessageList,
			NewSessionDialog,
			ResetIdentityDialog,
			RunnerAddedNotice,
			RevokeConfirmDialog,
			SearchPanel,
			SlashMenu,
			StatusBar,
			SwitchAgentDialog,
		},
		setup() {
			const trust = useTrustStore();
			const pairing = usePairingStore();
			return {
				store: useManagerStore(),
				RIGHT_PANEL_HIDE_AT,
				trust,
				pairing,
				remotePolicy: useRemotePolicyStore(),
				followers: useFollowersStore(async () => {
					await pairing.refreshMachines();
					await trust.refresh();
				}),
			};
		},
		computed: {
			leftW(): number {
				return clampWidth(
					'left',
					this.wantLeft,
					this.viewport,
					this.rightWant(),
				);
			},
			rightW(): number {
				return clampWidth('right', this.wantRight, this.viewport, this.leftW);
			},
			shellStyle(): Record<string, string> {
				return {
					gridTemplateColumns: shellColumns(
						this.leftW,
						this.rightW,
						this.viewport,
					),
				};
			},
			gateBlockers(): string[] {
				return this.trust.trust?.gate_blockers ?? this.pairing.blockers;
			},
			sharedWithPhone(): boolean {
				return sharedWithPhone(this.trust.trust?.devices);
			},
			revokeDevice() {
				return (
					this.trust.trust?.devices.find(
						(d) => d.id === this.trust.revokeTarget,
					) ?? null
				);
			},
			confirmKindDevice() {
				return (
					this.trust.trust?.devices.find(
						(d) => d.id === this.trust.confirmKindTarget?.id,
					) ?? null
				);
			},
			managerKey(): string | null {
				const rooms = this.store.rooms.filter((r) => r.room.kind === 'manager');
				const active = this.store.activeKey;
				const pick =
					rooms.find((r) => roomKey(r.room) === active) ??
					rooms.find((r) => r.room.machine === this.activeInfo?.room.machine) ??
					rooms[0];
				return pick ? roomKey(pick.room) : null;
			},
			searchScope(): 'chat' | 'all' | 'manager' {
				const key = this.store.search.scopeKey;
				if (!key) return 'all';
				if (key.startsWith('manager:') && key !== this.store.activeKey)
					return 'manager';
				return 'chat';
			},
			activeInfo() {
				return this.store.rooms.find(
					(r) => roomKey(r.room) === this.store.activeKey,
				);
			},
			deliveryWho(): string {
				const info = this.activeInfo;
				if (!info) return '';
				return info.agent
					? `${info.agent} on ${info.room.machine}`
					: info.room.machine;
			},
			deliveryMachine(): string {
				const id = this.activeInfo?.room.machine;
				if (!id) return '';
				const known = this.pairing.machines.find((m) => m.machine === id);
				return known?.label || id.slice(0, 12);
			},
			subtitle(): string {
				return this.activeInfo
					? headerSubtitle(this.activeInfo, this.store.rooms, (k, n) =>
							this.$t(k, { count: n }, n),
					  )
					: '';
			},
			placeholder(): string {
				return composerPlaceholder(this.activeInfo, (k, p) =>
					this.$t(k, p ?? {}),
				);
			},
			hint() {
				return (
					slashHint(this.store.slash.picked, this.store.draft.text) ?? undefined
				);
			},
			live(): boolean {
				const key = this.store.activeKey;
				return !!key && this.store.activityByRoom[key]?.state === 'working';
			},
			messages() {
				return this.store.activeKey
					? this.store.messagesByRoom[this.store.activeKey] ?? []
					: [];
			},
			permitGroupByMessage() {
				return permitGroups(this.messages);
			},
			pollGroupByMessage() {
				return pollGroups(this.messages);
			},
			shownMessages() {
				const groups = this.permitGroupByMessage;
				const polls = this.pollGroupByMessage;
				return this.messages.filter((m) => {
					if (pollHidden(m, polls)) return false;
					const d = m.decision;
					if (!d?.card_id) return true;
					return d.permit ? !!groups[m.id] : !d.closed;
				});
			},
			bodyHtml(): Record<string, string> {
				return Object.fromEntries(
					this.messages.map((m) => [m.id, renderBody(m.body_md)]),
				);
			},
			roomTitles(): Record<string, string> {
				return Object.fromEntries(
					this.store.rooms.map((r) => [roomKey(r.room), r.title]),
				);
			},
		},
		data() {
			return {
				wantLeft: loadWidth('left'),
				wantRight: loadWidth('right'),
				viewport: window.innerWidth,
				unlistenResync: null as (() => void) | null,
				stopWatch: null as (() => void) | null,
				stopMachines: null as (() => void) | null,
			};
		},
		mounted() {
			void this.store.init();
			void this.trust.init();
			void this.pairing.start().then(() => this.trust.refresh());
			void this.followers.refresh();
			void this.remotePolicy.refresh();
			void externalVoice.load();
			this.stopMachines = this.pairing.watch(5000);
			window.addEventListener('keydown', this.onGlobalKey);
			window.addEventListener('focus', this.onWindowFocus);
			window.addEventListener('resize', this.onWindowResize);
			this.stopWatch = this.store.watchRooms(5000);
			void onManagerResync(() => void this.store.refreshActive()).then(
				(off) => {
					this.unlistenResync = off;
				},
			);
		},
		beforeUnmount() {
			this.unlistenResync?.();
			this.stopWatch?.();
			this.store.stopQueuePoll();
			this.stopMachines?.();
			window.removeEventListener('keydown', this.onGlobalKey);
			window.removeEventListener('focus', this.onWindowFocus);
			window.removeEventListener('resize', this.onWindowResize);
			this.trust.dispose();
		},
		methods: {
			revokeAddedRunner() {
				const id = this.pairing.notice?.deviceId;
				this.pairing.dismiss();
				if (id) this.trust.revoke(id);
			},
			rightWant(): number {
				return clampWidth(
					'right',
					this.wantRight,
					this.viewport,
					DEFAULT_WIDTH.left,
				);
			},
			onWindowResize() {
				this.viewport = window.innerWidth;
			},
			startResize(side: PanelSide, e: PointerEvent) {
				e.preventDefault();
				const el = e.currentTarget as HTMLElement;
				el.setPointerCapture?.(e.pointerId);
				const move = (ev: PointerEvent) => {
					if (ev.buttons === 0) return;
					const x =
						side === 'left' ? ev.clientX : window.innerWidth - ev.clientX;
					if (side === 'left') this.wantLeft = x;
					else this.wantRight = x;
				};
				const up = () => {
					el.removeEventListener('pointermove', move);
					el.removeEventListener('pointerup', up);
					el.removeEventListener('pointercancel', up);
					el.removeEventListener('lostpointercapture', up);
					this.wantLeft = this.leftW;
					this.wantRight = this.rightW;
					saveWidth(side, side === 'left' ? this.leftW : this.rightW);
				};
				el.addEventListener('pointermove', move);
				el.addEventListener('pointerup', up);
				el.addEventListener('pointercancel', up);
				el.addEventListener('lostpointercapture', up);
			},
			resetWidth(side: PanelSide) {
				if (side === 'left') this.wantLeft = DEFAULT_WIDTH.left;
				else this.wantRight = DEFAULT_WIDTH.right;
				saveWidth(side, DEFAULT_WIDTH[side]);
			},
			onWindowFocus() {
				void this.store.loadRooms().catch(() => undefined);
			},
			onGlobalKey(e: KeyboardEvent) {
				if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
					e.preventDefault();
					this.store.openSearch();
				} else if (e.key === 'Escape' && this.store.search.open) {
					this.store.closeSearch();
				}
			},
			openRoom(room: RoomRef) {
				void this.store.select(roomKey(room));
			},
			onKey(e: KeyboardEvent) {
				if (this.store.mention.open) {
					if (e.key === 'ArrowDown') {
						e.preventDefault();
						this.store.moveMention(1);
					} else if (e.key === 'ArrowUp') {
						e.preventDefault();
						this.store.moveMention(-1);
					} else if (e.key === 'Tab' || e.key === 'Enter') {
						e.preventDefault();
						this.store.pickMention();
					} else if (e.key === 'Escape') {
						this.store.closeMention();
					}
					return;
				}
				if (!this.store.slash.open) return;
				if (e.key === 'ArrowDown') {
					e.preventDefault();
					this.store.moveSlash(1);
				} else if (e.key === 'ArrowUp') {
					e.preventDefault();
					this.store.moveSlash(-1);
				} else if (e.key === 'Tab' || e.key === 'Enter') {
					e.preventDefault();
					this.store.pickSlash();
				} else if (e.key === 'Escape') {
					this.store.closeSlash();
				}
			},
			onScope(scope: 'chat' | 'all' | 'manager') {
				this.store.search.scopeKey =
					scope === 'chat'
						? this.store.activeKey
						: scope === 'manager'
						? this.managerKey
						: null;
				void this.store.runSearch(this.store.search.text);
			},
			async onHit(hit: Parameters<typeof this.store.jumpToHit>[0]) {
				this.store.closeSearch();
				await this.store.jumpToHit(hit);
			},
			copy(text: string) {
				void navigator.clipboard?.writeText(text);
			},
			pick() {
				(this.$refs.file as HTMLInputElement).click();
			},
			onFiles(e: Event) {
				const input = e.target as HTMLInputElement;
				void this.addFiles(Array.from(input.files ?? []));
				input.value = '';
			},
			async addFiles(files: File[]) {
				for (const file of files) {
					if (file.size > MAX_FILE_BYTES) {
						this.store.error = `${file.name} is larger than 25 MiB`;
						continue;
					}
					this.store.addAttachment({
						name: file.name,
						type: file.type || 'application/octet-stream',
						size: file.size,
						sha256: await sha256Hex(file),
						blob: file,
					});
				}
			},
		},
	});
</script>

<style src="./manager.css"></style>
