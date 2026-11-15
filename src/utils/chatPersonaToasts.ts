import type { ChatMessage } from '@/actions/tmgr/chats';
import { toast } from '@/components/ui/toast';
import { t } from '@/i18n';
import { personaRefusalKey, refusedReplies } from '@/utils/chatPersonas';

export const reportPersonaRefusals = (message: ChatMessage | void) => {
	if (!message) return;
	refusedReplies(message.persona_replies).forEach((reply) =>
		toast({
			title: t('chats.persona.refusalTitle'),
			description: t(personaRefusalKey(reply.reason)),
			variant: 'destructive',
		}),
	);
};
