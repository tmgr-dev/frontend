import type { Mailbox } from '@/phones/types';
import $axios from '@/plugins/axios';

export const createPhoneMailbox = async (): Promise<Mailbox> => {
	const {
		data: { data },
	} = await $axios.post('phone-relay/mailboxes', {});
	return data;
};
