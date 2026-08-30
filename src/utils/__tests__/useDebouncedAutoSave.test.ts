import { nextTick, ref } from 'vue';
import { useDebouncedAutoSave } from '@/composable/useDebouncedAutoSave';

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
