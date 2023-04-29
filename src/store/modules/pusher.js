import pusher from 'src/store/plugins/pusher';
import pusherBeamsClient from 'src/store/plugins/pusher-beams-client';
import pusherTokenProvider from 'src/store/plugins/pusher-token-provider';

// @todo think how to avoid this repeating of getting token
const token = localStorage.getItem('token')
	? JSON.parse(localStorage.getItem('token'))
	: null;

const pusherModule = {
	state: () => ({
		pusher: pusher(token),
		pusherBeamsUserId: null,
		pusherBeamsClient: pusherBeamsClient,
		pusherTokenProvider: pusherTokenProvider(token),
	}),
	getters: {
		getPusherBeamsUserId: (state) => state.pusherBeamsUserId,
		getPusherBeamsClient: (state) => state.pusherBeamsClient,
		getPusherTokenProvider: (state) => state.pusherTokenProvider,
		getPusher: (state) => state.pusher,
	},
	mutations: {
		setPusherBeamsUserId(state, pusherBeamsUserId) {
			state.pusherBeamsUserId = pusherBeamsUserId;
		},
	},
};

export default pusherModule;
