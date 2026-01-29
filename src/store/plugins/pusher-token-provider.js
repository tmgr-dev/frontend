export default function (token) {
	return {
		token: token ? token.token : null,
		fetchToken: async () => {
			return { token: token ? token.token : null };
		},
	};
}
