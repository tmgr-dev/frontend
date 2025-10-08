import { ref, onUnmounted, nextTick } from 'vue';
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
  events: EventHandlers;
  isPrivate: boolean;
}

// Pusher configuration interface
interface PusherConfig {
  key: string;
  cluster: string;
  authEndpoint: string;
  forceTLS: boolean;
  auth: {
    headers: {
      Accept: string;
      Authorization: string;
    };
  };
}

/**
 * Pusher composable for managing real-time connections and events
 * Provides connection management, channel subscriptions, and event handling
 */
export function usePusher(): UsePusherReturn {
  // Connection state
  const isConnected = ref(false);
  const connectionState = ref<ConnectionState>('disconnected');
  const connectionError = ref<ActionError | null>(null);
  
  // Echo instance and subscriptions
  let echoInstance: Echo | null = null;
  const subscriptions = new Map<string, ChannelSubscription>();
  
  // Reconnection configuration
  const reconnectConfig = {
    maxAttempts: 5,
    delay: 1000,
    exponentialBackoff: true,
    currentAttempt: 0
  };

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

  // Create Pusher configuration
  const createPusherConfig = (): PusherConfig => {
    const token = getAuthToken();
    
    return {
      key: (import.meta as any).env.VITE_PUSHER_KEY,
      cluster: (import.meta as any).env.VITE_PUSHER_CLUSTER,
      authEndpoint: (import.meta as any).env.VITE_API_BASE_URL + 'broadcasting/auth',
      forceTLS: true,
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

  // Subscribe to a channel with event handlers
  const subscribe = (channelName: string, events: EventHandlers): void => {
    if (!echoInstance) {
      initializeEcho();
    }

    if (!echoInstance) {
      console.error('Failed to initialize Echo for subscription');
      return;
    }

    try {
      // Determine if it's a private channel
      const isPrivate = channelName.startsWith('private-') || 
                       channelName.includes('workspace.') || 
                       channelName.includes('user.');

      // Create channel subscription
      const channel = isPrivate 
        ? echoInstance.private(channelName)
        : echoInstance.channel(channelName);

      // Set up event listeners
      if (events.onActivityCreated) {
        channel.listen('ActivityCreated', (data: { activity: Activity }) => {
          events.onActivityCreated?.(data.activity);
        });
      }

      if (events.onDashboardUpdated) {
        channel.listen('DashboardUpdated', (data: { statistics: Partial<DashboardStatistics> }) => {
          events.onDashboardUpdated?.(data.statistics);
        });
      }

      if (events.onTaskUpdated) {
        channel.listen('TaskUpdated', (data: { task: RecentTask }) => {
          events.onTaskUpdated?.(data.task);
        });
      }

      if (events.onMemberStatusChanged) {
        channel.listen('MemberStatusChanged', (data: { member: TeamMemberStatus }) => {
          events.onMemberStatusChanged?.(data.member);
        });
      }

      // Handle connection errors
      if (events.onError) {
        channel.error((error: any) => {
          const actionError: ActionError = {
            message: error.message || 'Channel subscription error',
            type: 'network',
            timestamp: new Date().toISOString(),
            recoverable: true,
            details: error
          };
          events.onError?.(actionError);
        });
      }

      // Handle reconnection
      if (events.onReconnect) {
        channel.subscribed(() => {
          events.onReconnect?.();
        });
      }

      // Store subscription
      subscriptions.set(channelName, {
        channel,
        events,
        isPrivate
      });

      console.log(`Subscribed to channel: ${channelName}`);

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
    }
  };

  // Unsubscribe from a channel
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
        
        // Resubscribe to all channels
        const channelsToResubscribe = Array.from(subscriptions.entries());
        subscriptions.clear();
        
        for (const [channelName, subscription] of channelsToResubscribe) {
          subscribe(channelName, subscription.events);
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

  // Subscribe to workspace-specific events
  const subscribeToWorkspace = (workspaceId: number, events: EventHandlers): void => {
    const channelName = `workspace.${workspaceId}`;
    subscribe(channelName, events);
  };

  // Subscribe to user-specific events
  const subscribeToUser = (userId: number, events: EventHandlers): void => {
    const channelName = `user.${userId}`;
    subscribe(channelName, events);
  };

  // Unsubscribe from workspace events
  const unsubscribeFromWorkspace = (workspaceId: number): void => {
    const channelName = `workspace.${workspaceId}`;
    unsubscribe(channelName);
  };

  // Unsubscribe from user events
  const unsubscribeFromUser = (userId: number): void => {
    const channelName = `user.${userId}`;
    unsubscribe(channelName);
  };

  // Auto-cleanup on unmount
  onUnmounted(() => {
    disconnect();
  });

  // Initialize connection immediately
  initializeEcho();

  return {
    // Connection state
    isConnected: isConnected as Ref<boolean>,
    connectionState: connectionState as Ref<ConnectionState>,
    connectionError: connectionError as Ref<ActionError | null>,
    
    // Actions
    subscribe,
    unsubscribe,
    unsubscribeAll,
    reconnect,
    disconnect,
    emit,
    
    // Convenience methods
    subscribeToWorkspace,
    subscribeToUser,
    unsubscribeFromWorkspace,
    unsubscribeFromUser,
    
    // Utility
    getConnectionInfo
  };
}