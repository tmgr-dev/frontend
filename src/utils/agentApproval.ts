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

const REPO_NAME = /^[a-z0-9][a-z0-9-]{0,38}\/[a-z0-9._-]{1,100}$/;
export const MAX_REPOSITORIES = 5;

export const canonicalRepos = (names: readonly string[]): string[] => {
	if (names.length === 0 || names.length > MAX_REPOSITORIES)
		throw new Error('repository list must have 1 to 5 names');
	for (const name of names) {
		if (
			typeof name !== 'string' ||
			!REPO_NAME.test(name) ||
			name.endsWith('.git')
		)
			throw new Error(`invalid repository name: ${name}`);
	}
	if (new Set(names).size !== names.length)
		throw new Error('duplicate repository name');
	return [...names].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
};

export const reposSha256 = async (names: readonly string[]): Promise<string> =>
	sha256Hex(`tmgr-repos:v1:${canonicalRepos(names).join('\n')}`);

export const approvalMessage = (
	host: string,
	userId: number,
	workspaceId: number,
	taskId: number,
	categoryId: number | null | undefined,
	sha: string,
	keyId: string,
	signedAt: number,
	reposSha?: string | null,
): string =>
	reposSha
		? `tmgr-agent-approval:v2:${host}:${userId}:${workspaceId}:${taskId}:${
				categoryId ?? 0
		  }:${sha}:${reposSha}:${keyId}:${signedAt}`
		: `tmgr-agent-approval:v1:${host}:${userId}:${workspaceId}:${taskId}:${
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
