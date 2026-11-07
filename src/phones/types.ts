export interface PhoneView {
	id: string;
	name: string;
	fingerprint: string;
	paired_at_ms: number;
	last_used_ms: number;
	host: string;
}

export type PairingState = 'waiting' | 'confirm' | 'failed' | 'done';

export interface PairingView {
	state: PairingState;
	qr_svg_data_url: string;
	expires_ms: number;
	attempts_left: number;
	phone_name?: string;
	error?: string;
}

export interface BeginPairingArgs {
	apiBase: string;
	userId: number;
	mailboxId: string;
	desktopToken: string;
	phoneToken: string;
	desktopName: string;
}

export interface Mailbox {
	id: string;
	desktop_token: string;
	phone_token: string;
}
