import { createTourBus } from '../events';
import {
	emitPageLinkAdded,
	emitRoutineEditorOpened,
} from '../pageRoutineEvents';

describe('pageRoutineEvents', () => {
	it('emits page.link_added with the picked kind', () => {
		const bus = createTourBus();
		const seen: unknown[] = [];
		bus.on('page.link_added', (payload) => seen.push(payload));
		emitPageLinkAdded('task', bus);
		emitPageLinkAdded('page', bus);
		expect(seen).toEqual([{ kind: 'task' }, { kind: 'page' }]);
	});

	it('emits routine.editor_opened only for an existing routine', () => {
		const bus = createTourBus();
		let count = 0;
		bus.on('routine.editor_opened', () => count++);
		emitRoutineEditorOpened(true, bus);
		expect(count).toBe(0);
		emitRoutineEditorOpened(false, bus);
		expect(count).toBe(1);
	});
});
