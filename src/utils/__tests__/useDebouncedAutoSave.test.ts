import { useDebouncedAutoSave } from '@/composable/useDebouncedAutoSave';
import { nextTick, ref } from 'vue';

jest.useFakeTimers();

const flush = async () => {
	await nextTick();
	await Promise.resolve();
};

describe('useDebouncedAutoSave', () => {
	it('re-schedules a save for changes made while a save is in flight', async () => {
		const form = ref({ title: 't', description: 'a' });
		const saved: string[] = [];
		let release!: () => void;
		const onSave = jest.fn(
			() =>
				new Promise<void>((resolve) => {
					saved.push(form.value.description);
					release = resolve;
				}),
		);
		useDebouncedAutoSave({
			formRef: form,
			fieldsToWatch: ['description'],
			onSave,
			delay: 100,
		});

		form.value.description = 'ab';
		await flush();
		jest.advanceTimersByTime(100);
		await flush();
		expect(onSave).toHaveBeenCalledTimes(1);

		// user keeps typing while the first save is still pending
		form.value.description = 'abc';
		await flush();
		release();
		await flush();
		await flush();

		jest.advanceTimersByTime(100);
		await flush();
		expect(onSave).toHaveBeenCalledTimes(2);
		expect(saved).toEqual(['ab', 'abc']);
	});
});

it('flushes the captured task on disposal even if another task becomes current', async () => {
	const { effectScope } = await import('vue');
	const scope = effectScope();
	const form = ref({ id: 1, title: 'A' });
	const saved: unknown[] = [];
	scope.run(() =>
		useDebouncedAutoSave({
			formRef: form,
			fieldsToWatch: ['title'],
			onSave: (data) => {
				saved.push(data);
			},
			delay: 100,
		}),
	);
	form.value.title = 'A edited';
	scope.stop();
	form.value = { id: 2, title: 'B' };
	await flush();
	expect(saved).toEqual([{ id: 1, title: 'A edited' }]);
	jest.advanceTimersByTime(1000);
	await flush();
	expect(saved).toHaveLength(1);
});

it('serializes manual flush behind an in-flight save and preserves snapshots', async () => {
	const form = ref({ id: 1, title: 'A' });
	let release!: () => void;
	const saved: any[] = [];
	const save = jest.fn(async (snapshot) => {
		saved.push(snapshot);
		if (saved.length === 1)
			await new Promise<void>((resolve) => {
				release = resolve;
			});
	});
	const [, , flushSave] = useDebouncedAutoSave({
		formRef: form,
		fieldsToWatch: ['title'],
		onSave: save,
		delay: 100,
	});
	form.value.title = 'first';
	await flush();
	jest.advanceTimersByTime(100);
	await flush();
	form.value.title = 'second';
	await flush();
	const manual = flushSave(true);
	expect(save).toHaveBeenCalledTimes(1);
	release();
	await manual;
	expect(saved).toEqual([
		{ id: 1, title: 'first' },
		{ id: 1, title: 'second' },
	]);
});
