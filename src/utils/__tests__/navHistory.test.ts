import { createNavHistory, navDirectionForKey, navDirectionForMouse } from '../navHistory';

const e = (path: string, taskId: number | null = null) => ({ path, taskId });

describe('createNavHistory', () => {
	it('starts empty with nothing to navigate to', () => {
		const h = createNavHistory();
		expect(h.canBack()).toBe(false);
		expect(h.canForward()).toBe(false);
		expect(h.back()).toBeNull();
		expect(h.forward()).toBeNull();
	});

	it('walks back and forward through pushed entries', () => {
		const h = createNavHistory();
		h.push(e('/a'));
		h.push(e('/a', 1));
		h.push(e('/a', 2));
		expect(h.back()).toEqual(e('/a', 1));
		h.release();
		expect(h.back()).toEqual(e('/a'));
		h.release();
		expect(h.canBack()).toBe(false);
		expect(h.canForward()).toBe(true);
		expect(h.forward()).toEqual(e('/a', 1));
	});

	it('ignores consecutive duplicates', () => {
		const h = createNavHistory();
		h.push(e('/a'));
		h.push(e('/a'));
		h.push(e('/a', 1));
		h.push(e('/a', 1));
		expect(h.size()).toBe(2);
	});

	it('truncates the forward tail on a new push', () => {
		const h = createNavHistory();
		h.push(e('/a'));
		h.push(e('/b'));
		h.push(e('/c'));
		h.back();
		h.release();
		h.back();
		h.release();
		h.push(e('/d'));
		expect(h.canForward()).toBe(false);
		expect(h.back()).toEqual(e('/a'));
	});

	it('caps the stack and drops the oldest entries', () => {
		const h = createNavHistory(3);
		['/1', '/2', '/3', '/4'].forEach((p) => h.push(e(p)));
		expect(h.size()).toBe(3);
		h.back();
		h.release();
		expect(h.back()).toEqual(e('/2'));
		h.release();
		expect(h.canBack()).toBe(false);
	});

	it('records nothing while a back/forward navigation is being applied', () => {
		const h = createNavHistory();
		h.push(e('/a'));
		h.push(e('/b'));
		h.back();
		h.push(e('/side-effect'));
		expect(h.size()).toBe(2);
		h.release();
		h.push(e('/c'));
		expect(h.size()).toBe(2);
		expect(h.canForward()).toBe(false);
	});

	it('replace overwrites the current entry without growing the stack', () => {
		const h = createNavHistory();
		h.push(e('/board'));
		h.replace(e('/list'));
		expect(h.size()).toBe(1);
		h.push(e('/x'));
		expect(h.back()).toEqual(e('/list'));
	});

	it('replace that lands on the previous entry collapses into it', () => {
		const h = createNavHistory();
		h.push(e('/a'));
		h.push(e('/b'));
		h.replace(e('/a'));
		expect(h.size()).toBe(1);
		expect(h.canBack()).toBe(false);
	});

	it('does not move past the ends', () => {
		const h = createNavHistory();
		h.push(e('/a'));
		expect(h.back()).toBeNull();
		expect(h.forward()).toBeNull();
	});
});

describe('navDirectionForKey', () => {
	const key = (init: Partial<KeyboardEvent>) => init as KeyboardEvent;

	it('maps Cmd+[ and Cmd+] on macOS', () => {
		expect(navDirectionForKey(key({ metaKey: true, key: '[', code: 'BracketLeft' }), 'macos')).toBe('back');
		expect(navDirectionForKey(key({ metaKey: true, key: ']', code: 'BracketRight' }), 'macos')).toBe('forward');
	});

	it('maps Alt+Arrow on Windows and Linux', () => {
		expect(navDirectionForKey(key({ altKey: true, key: 'ArrowLeft' }), 'windows')).toBe('back');
		expect(navDirectionForKey(key({ altKey: true, key: 'ArrowRight' }), 'linux')).toBe('forward');
	});

	it('ignores other chords and extra modifiers', () => {
		expect(navDirectionForKey(key({ altKey: true, key: 'ArrowLeft' }), 'macos')).toBeNull();
		expect(navDirectionForKey(key({ metaKey: true, key: '[' }), 'windows')).toBeNull();
		expect(navDirectionForKey(key({ metaKey: true, shiftKey: true, key: '[' }), 'macos')).toBeNull();
		expect(navDirectionForKey(key({ key: '[' }), 'macos')).toBeNull();
	});
});

describe('navDirectionForMouse', () => {
	it('maps the side buttons', () => {
		expect(navDirectionForMouse(3)).toBe('back');
		expect(navDirectionForMouse(4)).toBe('forward');
		expect(navDirectionForMouse(0)).toBeNull();
		expect(navDirectionForMouse(1)).toBeNull();
	});
});
