#!/usr/bin/env node
// minisign-compatible signing with Node's own crypto (Ed25519 over a BLAKE2b-512 prehash, the "ED" algorithm).
//
//   node sign.mjs keygen <secret-key-file>        writes the secret key (mode 600) and <file>.pub
//   node sign.mjs pubkey <secret-key-file>        prints the public key line
//   node sign.mjs sign <secret-key-file> <file>   writes <file>.minisig
//
// The secret key may come from the TMGR_PLUGIN_SIGNING_KEY environment variable instead of a file:
// pass "-" as the file name. It is never printed.
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { basename } from 'node:path';

const blake2b = (data) => createHash('blake2b512').update(data).digest();

const loadSecret = (file) => {
	const pem = file === '-' ? process.env.TMGR_PLUGIN_SIGNING_KEY : readFileSync(file, 'utf8');
	if (!pem) throw new Error('no signing key: set TMGR_PLUGIN_SIGNING_KEY or pass a key file');
	return createPrivateKey(pem);
};

const rawPublicKey = (secret) =>
	Buffer.from(createPublicKey(secret).export({ format: 'jwk' }).x, 'base64url');

// minisign key ids are 8 bytes; derived from the public key so the PEM needs nothing extra.
const keyId = (publicKey) => blake2b(publicKey).subarray(0, 8);

const keyIdHex = (id) => Buffer.from(id).reverse().toString('hex').toUpperCase();

export const publicKeyLine = (secret) => {
	const pk = rawPublicKey(secret);
	return Buffer.concat([Buffer.from('Ed'), keyId(pk), pk]).toString('base64');
};

export const signBytes = (secret, data, trustedComment) => {
	const id = keyId(rawPublicKey(secret));
	const signature = sign(null, blake2b(data), secret);
	const global = sign(null, Buffer.concat([signature, Buffer.from(trustedComment)]), secret);
	return [
		`untrusted comment: signature from tmgr key ${keyIdHex(id)}`,
		Buffer.concat([Buffer.from('ED'), id, signature]).toString('base64'),
		`trusted comment: ${trustedComment}`,
		global.toString('base64'),
		'',
	].join('\n');
};

const main = ([command, keyFile, file]) => {
	if (command === 'keygen' && keyFile) {
		if (existsSync(keyFile)) throw new Error(`${keyFile} already exists`);
		const { privateKey } = generateKeyPairSync('ed25519');
		writeFileSync(keyFile, privateKey.export({ type: 'pkcs8', format: 'pem' }), { mode: 0o600 });
		const line = publicKeyLine(privateKey);
		const id = keyIdHex(keyId(rawPublicKey(privateKey)));
		writeFileSync(`${keyFile}.pub`, `untrusted comment: minisign public key ${id}\n${line}\n`);
		console.log(line);
	} else if (command === 'pubkey' && keyFile) {
		console.log(publicKeyLine(loadSecret(keyFile)));
	} else if (command === 'sign' && keyFile && file) {
		const data = readFileSync(file);
		const comment = `timestamp:${Math.floor(Date.now() / 1000)}\tfile:${basename(file)}\thashed`;
		writeFileSync(`${file}.minisig`, signBytes(loadSecret(keyFile), data, comment));
		console.log(`${file}.minisig`);
	} else {
		console.error('usage: sign.mjs keygen <key> | pubkey <key|-> | sign <key|-> <file>');
		process.exit(2);
	}
};

if (process.argv[1] && import.meta.url.endsWith(basename(process.argv[1]))) {
	main(process.argv.slice(2));
}
