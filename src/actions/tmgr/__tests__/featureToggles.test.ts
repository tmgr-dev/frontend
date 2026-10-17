import $axios from '@/plugins/axios';
import {
	getUserFeatureToggles,
	updateUserFeatureToggles,
} from '../featureToggles';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), put: jest.fn() },
}));

const http = $axios as unknown as { get: jest.Mock; put: jest.Mock };

beforeEach(() => {
	http.get.mockReset();
	http.put.mockReset();
});

describe('getUserFeatureToggles', () => {
	it('sends one request for concurrent reads', async () => {
		http.get.mockResolvedValue({ data: { data: { a: { key: 'a' } } } });
		const [first, second] = await Promise.all([
			getUserFeatureToggles(),
			getUserFeatureToggles(),
		]);
		expect(http.get).toHaveBeenCalledTimes(1);
		expect(first).toEqual(second);
	});

	it('does not reuse a finished read', async () => {
		http.get.mockResolvedValue({ data: { data: {} } });
		await getUserFeatureToggles();
		await getUserFeatureToggles();
		expect(http.get).toHaveBeenCalledTimes(2);
	});

	it('reads again after an update even when an older read is in flight', async () => {
		let release: (value: unknown) => void = () => {};
		http.get.mockReturnValueOnce(new Promise((resolve) => (release = resolve)));
		http.get.mockResolvedValueOnce({ data: { data: { fresh: {} } } });
		http.put.mockResolvedValue({ data: {} });
		const stale = getUserFeatureToggles();
		await updateUserFeatureToggles({ x: true });
		const fresh = await getUserFeatureToggles();
		expect(fresh).toEqual({ fresh: {} });
		release({ data: { data: { old: {} } } });
		await stale;
	});
});
