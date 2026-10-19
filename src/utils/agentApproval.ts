const encoder = new TextEncoder();

const hex = (buffer: ArrayBuffer): string =>
	Array.from(new Uint8Array(buffer), (b) =>
		b.toString(16).padStart(2, '0'),
	).join('');

const sha256Hex = async (text: string): Promise<string> =>
	hex(await globalThis.crypto.subtle.digest('SHA-256', encoder.encode(text)));

export const norm = (value: string | null | undefined): string =>
	(value ?? '').replace(/\r\n/g, '\n');

export const contentSha256 = async (
	title: string | null | undefined,
	description: string | null | undefined,
): Promise<string> =>
	sha256Hex(
		`tmgr-task-content:v1:${await sha256Hex(norm(title))}:${await sha256Hex(
			norm(description),
		)}`,
	);

export const approvalMessage = (
	host: string,
	userId: number,
	workspaceId: number,
	taskId: number,
	categoryId: number | null | undefined,
	sha: string,
	keyId: string,
	signedAt: number,
): string =>
	`tmgr-agent-approval:v1:${host}:${userId}:${workspaceId}:${taskId}:${
		categoryId ?? 0
	}:${sha}:${keyId}:${signedAt}`;

export const toBase64 = (bytes: ArrayBuffer | Uint8Array): string => {
	const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
	let binary = '';
	for (const byte of view) binary += String.fromCharCode(byte);
	return btoa(binary);
};

export const fromBase64 = (text: string): Uint8Array =>
	Uint8Array.from(atob(text), (c) => c.charCodeAt(0));

const ECDSA = { name: 'ECDSA', namedCurve: 'P-256' } as const;
const SIGN = { name: 'ECDSA', hash: 'SHA-256' } as const;

export const signMessage = async (
	privateKey: CryptoKey,
	message: string,
): Promise<string> =>
	toBase64(
		await globalThis.crypto.subtle.sign(
			SIGN,
			privateKey,
			encoder.encode(message),
		),
	);

export const verifyMessage = async (
	publicKeyBase64: string,
	message: string,
	signatureBase64: string,
): Promise<boolean> => {
	try {
		const key = await globalThis.crypto.subtle.importKey(
			'raw',
			fromBase64(publicKeyBase64),
			ECDSA,
			false,
			['verify'],
		);
		return await globalThis.crypto.subtle.verify(
			SIGN,
			key,
			fromBase64(signatureBase64),
			encoder.encode(message),
		);
	} catch {
		return false;
	}
};

export const generateSigningKeyPair = (): Promise<CryptoKeyPair> =>
	globalThis.crypto.subtle.generateKey(ECDSA, false, [
		'sign',
		'verify',
	]) as Promise<CryptoKeyPair>;

export const exportPublicKey = async (key: CryptoKey): Promise<string> =>
	toBase64(await globalThis.crypto.subtle.exportKey('raw', key));

export const apiHostOf = (apiBase: string, origin?: string): string =>
	new URL(apiBase, origin ?? 'http://localhost').host.toLowerCase();
