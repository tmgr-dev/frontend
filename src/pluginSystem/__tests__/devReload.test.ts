import { stepDevWatch, type DevWatchState } from '../devReload';

it('seeds a baseline on the first poll without restarting anything', () => {
	const { next, changed } = stepDevWatch(null, { a: 'h1', b: 'h2' });
	expect(changed).toEqual([]);
	expect(next).toEqual({ stable: { a: 'h1', b: 'h2' }, pending: {} });
});

it('does not restart on an unchanged fingerprint', () => {
	const seeded = stepDevWatch(null, { a: 'h1' }).next;
	const { next, changed } = stepDevWatch(seeded, { a: 'h1' });
	expect(changed).toEqual([]);
	expect(next).toEqual({ stable: { a: 'h1' }, pending: {} });
});

it('waits for a changed fingerprint to be seen twice before restarting', () => {
	let state: DevWatchState = stepDevWatch(null, { a: 'h1' }).next;

	const first = stepDevWatch(state, { a: 'h2' });
	expect(first.changed).toEqual([]);
	expect(first.next).toEqual({ stable: { a: 'h1' }, pending: { a: 'h2' } });
	state = first.next;

	const second = stepDevWatch(state, { a: 'h2' });
	expect(second.changed).toEqual(['a']);
	expect(second.next).toEqual({ stable: { a: 'h2' }, pending: {} });
});

it('does not restart when a change reverts before being confirmed', () => {
	const seeded = stepDevWatch(null, { a: 'h1' }).next;
	const midSave = stepDevWatch(seeded, { a: 'h2' }).next;
	const { next, changed } = stepDevWatch(midSave, { a: 'h1' });
	expect(changed).toEqual([]);
	expect(next).toEqual({ stable: { a: 'h1' }, pending: {} });
});

it('treats a new folder as pending, not an immediate restart', () => {
	const seeded = stepDevWatch(null, { a: 'h1' }).next;
	const { next, changed } = stepDevWatch(seeded, { a: 'h1', b: 'h2' });
	expect(changed).toEqual([]);
	expect(next.pending).toEqual({ b: 'h2' });
});

it('drops a removed folder from the baseline without reporting it as changed', () => {
	const seeded = stepDevWatch(null, { a: 'h1', b: 'h2' }).next;
	const { next, changed } = stepDevWatch(seeded, { a: 'h1' });
	expect(changed).toEqual([]);
	expect(next).toEqual({ stable: { a: 'h1' }, pending: {} });
});

it('handles several folders independently', () => {
	const seeded = stepDevWatch(null, { a: 'h1', b: 'h1' }).next;
	const settling = stepDevWatch(seeded, { a: 'h2', b: 'h1' }).next;
	const { changed, next } = stepDevWatch(settling, { a: 'h2', b: 'h1' });
	expect(changed).toEqual(['a']);
	expect(next.stable).toEqual({ a: 'h2', b: 'h1' });
});
