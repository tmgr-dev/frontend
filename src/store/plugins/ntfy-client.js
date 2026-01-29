class NtfyClient {
	constructor() {
		this.eventSource = null;
		this.userId = null;
		this.serverUrl = import.meta.env.VITE_NTFY_SERVER_URL || 'http://localhost:2586';
		this.defaultTopic = import.meta.env.VITE_NTFY_DEFAULT_TOPIC || 'tmgr-notifications';
		this.onMessageCallback = null;
	}

	getUserTopic() {
		return `${this.defaultTopic}-user-${this.userId}`;
	}

	async start() {
		return Promise.resolve();
	}

	async stop() {
		if (this.eventSource) {
			this.eventSource.close();
			this.eventSource = null;
		}
		this.userId = null;
		return Promise.resolve();
	}

	async setUserId(userId, tokenProvider = null) {
		this.userId = userId;
		await this.connect();
		return Promise.resolve();
	}

	async getUserId() {
		return Promise.resolve(this.userId);
	}

	async connect() {
		if (!this.userId) {
			return;
		}

		if (this.eventSource) {
			this.eventSource.close();
		}

		const topic = this.getUserTopic();
		const url = `${this.serverUrl}/${topic}/sse`;

		try {
			this.eventSource = new EventSource(url);

			this.eventSource.onmessage = (event) => {
				try {
					const data = JSON.parse(event.data);
					if (data.event === 'message' && this.onMessageCallback) {
						this.onMessageCallback({
							title: data.title || 'Notification',
							body: data.message || '',
							deepLink: data.click || null,
						});
					}

					if ('Notification' in window && Notification.permission === 'granted') {
						const notification = new Notification(data.title || 'TMGR Notification', {
							body: data.message || '',
							icon: '/favicon.ico',
						});

						if (data.click) {
							notification.onclick = () => {
								window.open(data.click, '_blank');
							};
						}
					}
				} catch (e) {
					console.error('Failed to parse ntfy message:', e);
				}
			};

			this.eventSource.onerror = (error) => {
				console.error('ntfy EventSource error:', error);
			};

			this.eventSource.onopen = () => {
				console.log('ntfy connection established for user:', this.userId);
			};
		} catch (error) {
			console.error('Failed to connect to ntfy:', error);
		}
	}

	onMessage(callback) {
		this.onMessageCallback = callback;
	}

	async requestPermission() {
		if ('Notification' in window) {
			const permission = await Notification.requestPermission();
			return permission === 'granted';
		}
		return false;
	}
}

let client = {};

if (typeof window !== 'undefined') {
	client = new NtfyClient();
}

export default client;
