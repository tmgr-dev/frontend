type PendingRequest<T> = {
	promise: Promise<T>;
	timestamp: number;
};

class RequestDeduplicator {
	private pendingRequests: Map<string, PendingRequest<any>>;
	private maxAge: number;

	constructor(maxAge: number = 5000) {
		this.pendingRequests = new Map();
		this.maxAge = maxAge;
	}

	async deduplicate<T>(
		key: string,
		requestFn: () => Promise<T>,
	): Promise<T> {
		const existing = this.pendingRequests.get(key);
		
		if (existing) {
			const age = Date.now() - existing.timestamp;
			if (age < this.maxAge) {
				return existing.promise;
			} else {
				this.pendingRequests.delete(key);
			}
		}

		const promise = requestFn()
			.finally(() => {
				setTimeout(() => {
					this.pendingRequests.delete(key);
				}, 100);
			});

		this.pendingRequests.set(key, {
			promise,
			timestamp: Date.now(),
		});

		return promise;
	}

	clear(): void {
		this.pendingRequests.clear();
	}

	has(key: string): boolean {
		return this.pendingRequests.has(key);
	}

	size(): number {
		return this.pendingRequests.size;
	}
}

export const requestDeduplicator = new RequestDeduplicator();

export default requestDeduplicator;
