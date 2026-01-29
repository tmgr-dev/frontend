import { ref, nextTick } from 'vue';
import type { Ref } from 'vue';
import Echo from 'laravel-echo';
import type {
  EventHandlers,
  Activity,
  DashboardStatistics,
  RecentTask,
  TeamMemberStatus,
  ActionError,
  UsePusherReturn
} from '@/types/dashboard';

// Connection state type
type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'error' | 'reconnecting';

// Channel subscription interface
interface ChannelSubscription {
  channel: any;
  handlers: Map<string, EventHandlers>;
  isPrivate: boolean;
}

// Generate unique subscription ID
let subscriptionIdCounter = 0;
const generateSubscriptionId = (): string => {
  return `sub_${++subscriptionIdCounter}_${Date.now()}`;
};

// Pusher configuration interface (Soketi compatible)
interface PusherConfig {
  key: string;
  cluster: string;
  wsHost: string;
  wsPort: number;
  wssPort: number;
  authEndpoint: string;
  forceTLS: boolean;
  disableStats: boolean;
  enabledTransports: string[];
  auth: {
    headers: {
      Accept: string;
      Authorization: string;
    };
  };
}

// Singleton state - shared across all usePusher instances
const isConnected = ref(false);
const connectionState = ref<ConnectionState>('disconnected');
const connectionError = ref<ActionError | null>(null);
let echoInstance: Echo | null = null;
const subscriptions = new Map<string, ChannelSubscription>();
const reconnectConfig = {
  maxAttempts: 5,
  delay: 1000,
  exponentialBackoff: true,
  currentAttempt: 0
};

/**
 * Pusher composable for managing real-time connections and events
 * Provides connection management, channel subscriptions, and event handling
 * Uses singleton pattern - all instances share the same Echo connection
 */
export function usePusher(): UsePusherReturn {

  // Get authentication token
  const getAuthToken = (): string | null => {
    try {
      const token = localStorage.getItem('token');
      if (token) {
        const parsedToken = JSON.parse(token);
        return parsedToken.token || null;
      }
    } catch (error) {
      console.error('Failed to parse auth token:', error);
    }
    return null;
  };

  // Create Pusher configuration (Soketi compatible)
  const createPusherConfig = (): PusherConfig => {
    const token = getAuthToken();
    const scheme = (import.meta as any).env.VITE_PUSHER_SCHEME || 'http';
    const port = parseInt((import.meta as any).env.VITE_PUSHER_PORT || '6001', 10);
    
    return {
      key: (import.meta as any).env.VITE_PUSHER_KEY,
      cluster: 'mt1',
      wsHost: (import.meta as any).env.VITE_PUSHER_HOST || 'localhost',
      wsPort: port,
      wssPort: port,
      authEndpoint: (import.meta as any).env.VITE_API_BASE_URL + 'broadcasting/auth',
      forceTLS: scheme === 'https',
      disableStats: true,
      enabledTransports: ['ws', 'wss'],
      auth: {
        headers: {
          Accept: 'application/json',
          Authorization: `Bearer ${token || ''}`
        }
      }
    };
  };

  // Initialize Echo connection
  const initializeEcho = async (): Promise<void> => {
    if (echoInstance) {
      return; // Already initialized
    }

    try {
      connectionState.value = 'connecting';
      connectionError.value = null;

      const config = createPusherConfig();
      
      // Set up Pusher globally (required by Laravel Echo)
      if (typeof window !== 'undefined') {
        const { default: Pusher } = await import('pusher-js');
        (window as any).Pusher = Pusher;
      }

      echoInstance = new Echo({
        broadcaster: 'pusher',
        ...config
      });

      // Set up connection event listeners
      setupConnectionListeners();
      
    } catch (error) {
      console.error('Failed to initialize Echo:', error);
      connectionState.value = 'error';
      connectionError.value = {
        message: error instanceof Error ? error.message : 'Failed to initialize Pusher connection',
        type: 'network',
        timestamp: new Date().toISOString(),
        recoverable: true
      };
    }
  };

  // Set up connection event listeners
  const setupConnectionListeners = (): void => {
    if (!echoInstance) return;

    // Access the underlying Pusher connection
    const pusher = (echoInstance as any).connector.pusher;

    if (pusher) {
      pusher.connection.bind('connected', () => {
        isConnected.value = true;
        connectionState.value = 'connected';
        connectionError.value = null;
        reconnectConfig.currentAttempt = 0;
        console.log('Pusher connected successfully');
      });

      pusher.connection.bind('disconnected', () => {
        isConnected.value = false;
        connectionState.value = 'disconnected';
        console.log('Pusher disconnected');
      });

      pusher.connection.bind('error', (error: any) => {
        isConnected.value = false;
        connectionState.value = 'error';
        connectionError.value = {
          message: error.error?.message || 'Connection error occurred',
          type: 'network',
          timestamp: new Date().toISOString(),
          recoverable: true,
          details: error
        };
        console.error('Pusher connection error:', error);
        
        // Attempt reconnection
        scheduleReconnect();
      });

      pusher.connection.bind('unavailable', () => {
        isConnected.value = false;
        connectionState.value = 'error';
        console.warn('Pusher connection unavailable');
        scheduleReconnect();
      });
    }
  };

  // Schedule reconnection attempt
  const scheduleReconnect = (): void => {
    if (reconnectConfig.currentAttempt >= reconnectConfig.maxAttempts) {
      console.error('Max reconnection attempts reached');
      return;
    }

    reconnectConfig.currentAttempt++;
    connectionState.value = 'reconnecting';

    const delay = reconnectConfig.exponentialBackoff
      ? reconnectConfig.delay * Math.pow(2, reconnectConfig.currentAttempt - 1)
      : reconnectConfig.delay;

    setTimeout(() => {
      console.log(`Attempting to reconnect (${reconnectConfig.currentAttempt}/${reconnectConfig.maxAttempts})`);
      reconnect();
    }, delay);
  };

  // Subscribe to a channel with event handlers - returns subscription ID for unsubscribing
  const subscribe = (channelName: string, events: EventHandlers): string => {
    console.log(`[usePusher] subscribe called for channel: ${channelName}, echoInstance exists: ${!!echoInstance}`);
    
    if (!echoInstance) {
      initializeEcho();
    }

    if (!echoInstance) {
      console.error('Failed to initialize Echo for subscription');
      return '';
    }

    const subscriptionId = generateSubscriptionId();
    console.log(`[usePusher] Generated subscriptionId: ${subscriptionId}`);

    try {
      // Check if we already have a subscription to this channel
      const existingSubscription = subscriptions.get(channelName);
      console.log(`[usePusher] Existing subscription: ${!!existingSubscription}, total subscriptions: ${subscriptions.size}`);
      
      if (existingSubscription) {
        // Add new handler to existing channel subscription
        existingSubscription.handlers.set(subscriptionId, events);
        console.log(`[usePusher] Added handler ${subscriptionId} to existing channel: ${channelName}, total handlers: ${existingSubscription.handlers.size}`);
        return subscriptionId;
      }

      // Determine if it's a private channel
      const isPrivate = channelName.startsWith('private-') || 
                       channelName.includes('App.Workspace.') || 
                       channelName.includes('App.User.');

      // Create channel subscription
      const channel = isPrivate 
        ? echoInstance.private(channelName)
        : echoInstance.channel(channelName);

      // Create handlers map and add the first handler
      const handlers = new Map<string, EventHandlers>();
      handlers.set(subscriptionId, events);

      // Set up event listeners that dispatch to ALL handlers
      channel.listen('.activity.created', (data: { activity: Activity }) => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onActivityCreated?.(data.activity));
        }
      });

      channel.listen('.dashboard.updated', (data: { statistics: Partial<DashboardStatistics> }) => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onDashboardUpdated?.(data.statistics));
        }
      });

      channel.listen('.task.updated', (data: { task: any, action: 'created' | 'updated' | 'deleted', updated_by_user_id: number, source_instance_id?: string }) => {
        console.log('[usePusher] Received .task.updated event:', data);
        const sub = subscriptions.get(channelName);
        console.log('[usePusher] Found subscription:', !!sub, 'handlers count:', sub?.handlers?.size);
        if (sub) {
          sub.handlers.forEach((h, id) => {
            console.log('[usePusher] Calling handler:', id);
            h.onTaskUpdated?.(data.task, data.action, data.updated_by_user_id, data.source_instance_id);
          });
        }
      });

      channel.listen('.comment-added', (data: { comment: any }) => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onCommentAdded?.(data.comment));
        }
      });

      channel.listen('.comment-updated', (data: { comment: any }) => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onCommentUpdated?.(data.comment));
        }
      });

      channel.listen('.comment-deleted', (data: { comment: any }) => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onCommentDeleted?.(data.comment));
        }
      });

      channel.listen('.member.status.changed', (data: { member: TeamMemberStatus }) => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onMemberStatusChanged?.(data.member));
        }
      });

      channel.listen('.notification.created', (data: any) => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onNotificationCreated?.(data));
        }
      });

      // Handle connection errors
      channel.error((error: any) => {
        const actionError: ActionError = {
          message: error.message || 'Channel subscription error',
          type: 'network',
          timestamp: new Date().toISOString(),
          recoverable: true,
          details: error
        };
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onError?.(actionError));
        }
      });

      // Handle reconnection
      channel.subscribed(() => {
        const sub = subscriptions.get(channelName);
        if (sub) {
          sub.handlers.forEach(h => h.onReconnect?.());
        }
      });

      // Store subscription with handlers map
      subscriptions.set(channelName, {
        channel,
        handlers,
        isPrivate
      });

      console.log(`Subscribed to channel: ${channelName} with handler ${subscriptionId}`);
      return subscriptionId;

    } catch (error) {
      console.error(`Failed to subscribe to channel ${channelName}:`, error);
      
      if (events.onError) {
        const actionError: ActionError = {
          message: error instanceof Error ? error.message : 'Subscription failed',
          type: 'server',
          timestamp: new Date().toISOString(),
          recoverable: true
        };
        events.onError(actionError);
      }
      return '';
    }
  };

  // Unsubscribe a specific handler from a channel
  const unsubscribeHandler = (channelName: string, subscriptionId: string): void => {
    const subscription = subscriptions.get(channelName);
    
    if (subscription) {
      subscription.handlers.delete(subscriptionId);
      console.log(`Removed handler ${subscriptionId} from channel: ${channelName}`);
      
      // If no more handlers, leave the channel entirely
      if (subscription.handlers.size === 0 && echoInstance) {
        try {
          echoInstance.leave(channelName);
          subscriptions.delete(channelName);
          console.log(`Left channel: ${channelName} (no more handlers)`);
        } catch (error) {
          console.error(`Failed to leave channel ${channelName}:`, error);
        }
      }
    }
  };

  // Unsubscribe from a channel entirely (removes all handlers)
  const unsubscribe = (channelName: string): void => {
    const subscription = subscriptions.get(channelName);
    
    if (subscription && echoInstance) {
      try {
        // Leave the channel
        echoInstance.leave(channelName);
        
        // Remove from subscriptions map
        subscriptions.delete(channelName);
        
        console.log(`Unsubscribed from channel: ${channelName}`);
      } catch (error) {
        console.error(`Failed to unsubscribe from channel ${channelName}:`, error);
      }
    }
  };

  // Unsubscribe from all channels
  const unsubscribeAll = (): void => {
    for (const channelName of subscriptions.keys()) {
      unsubscribe(channelName);
    }
  };

  // Reconnect to Pusher
  const reconnect = (): void => {
    try {
      // Disconnect existing connection
      disconnect();
      
      // Wait a moment before reconnecting
      nextTick(() => {
        // Reinitialize Echo
        initializeEcho();
        
        // Resubscribe to all channels with all their handlers
        const channelsToResubscribe = Array.from(subscriptions.entries());
        subscriptions.clear();
        
        for (const [channelName, subscription] of channelsToResubscribe) {
          // Resubscribe each handler individually
          for (const [, handler] of subscription.handlers) {
            subscribe(channelName, handler);
          }
        }
      });
      
    } catch (error) {
      console.error('Failed to reconnect:', error);
      connectionState.value = 'error';
    }
  };

  // Disconnect from Pusher
  const disconnect = (): void => {
    if (echoInstance) {
      try {
        // Unsubscribe from all channels
        unsubscribeAll();
        
        // Disconnect Echo
        echoInstance.disconnect();
        echoInstance = null;
        
        isConnected.value = false;
        connectionState.value = 'disconnected';
        
        console.log('Pusher disconnected');
      } catch (error) {
        console.error('Error during disconnect:', error);
      }
    }
  };

  // Emit custom event (for testing or custom events)
  const emit = (event: string, data: any): void => {
    if (!echoInstance || !isConnected.value) {
      console.warn('Cannot emit event: Pusher not connected');
      return;
    }

    try {
      // This would typically be used for client events
      // Implementation depends on your specific use case
      console.log(`Emitting event: ${event}`, data);
    } catch (error) {
      console.error(`Failed to emit event ${event}:`, error);
    }
  };

  // Get connection status information
  const getConnectionInfo = () => ({
    isConnected: isConnected.value,
    state: connectionState.value,
    error: connectionError.value,
    subscriptions: Array.from(subscriptions.keys()),
    reconnectAttempts: reconnectConfig.currentAttempt
  });

  // Subscribe to workspace-specific events - returns subscription ID
  const subscribeToWorkspace = (workspaceId: number, events: EventHandlers): string => {
    const channelName = `App.Workspace.${workspaceId}`;
    return subscribe(channelName, events);
  };

  // Subscribe to user-specific events - returns subscription ID
  const subscribeToUser = (userId: number, events: EventHandlers): string => {
    const channelName = `App.User.${userId}`;
    return subscribe(channelName, events);
  };

  // Unsubscribe a specific handler from workspace events
  const unsubscribeHandlerFromWorkspace = (workspaceId: number, subscriptionId: string): void => {
    const channelName = `App.Workspace.${workspaceId}`;
    unsubscribeHandler(channelName, subscriptionId);
  };

  // Unsubscribe from workspace events entirely (all handlers)
  const unsubscribeFromWorkspace = (workspaceId: number): void => {
    const channelName = `App.Workspace.${workspaceId}`;
    unsubscribe(channelName);
  };

  // Unsubscribe from user events
  const unsubscribeFromUser = (userId: number): void => {
    const channelName = `App.User.${userId}`;
    unsubscribe(channelName);
  };

  // Initialize connection if not already connected (singleton pattern)
  if (!echoInstance) {
    initializeEcho();
  }

  return {
    // Connection state
    isConnected: isConnected as Ref<boolean>,
    connectionState: connectionState as Ref<ConnectionState>,
    connectionError: connectionError as Ref<ActionError | null>,
    
    // Actions
    subscribe,
    unsubscribe,
    unsubscribeHandler,
    unsubscribeAll,
    reconnect,
    disconnect,
    emit,
    
    // Convenience methods
    subscribeToWorkspace,
    subscribeToUser,
    unsubscribeFromWorkspace,
    unsubscribeHandlerFromWorkspace,
    unsubscribeFromUser,
    
    // Utility
    getConnectionInfo
  };
}