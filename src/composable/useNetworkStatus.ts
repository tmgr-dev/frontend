import { ref, onMounted, onUnmounted } from 'vue';
import type { Ref } from 'vue';

export interface NetworkStatus {
  isOnline: boolean;
  isOffline: boolean;
  effectiveType?: string;
  downlink?: number;
  rtt?: number;
}

export interface UseNetworkStatusReturn {
  isOnline: Ref<boolean>;
  isOffline: Ref<boolean>;
  networkStatus: Ref<NetworkStatus>;
  checkConnection: () => Promise<boolean>;
  onOnline: (callback: () => void) => void;
  onOffline: (callback: () => void) => void;
}

/**
 * Network status composable for monitoring online/offline state
 * Provides reactive network status and connection monitoring
 */
export function useNetworkStatus(): UseNetworkStatusReturn {
  const isOnline = ref(navigator.onLine);
  const isOffline = ref(!navigator.onLine);
  
  const networkStatus = ref<NetworkStatus>({
    isOnline: navigator.onLine,
    isOffline: !navigator.onLine
  });

  // Callback arrays for event handling
  const onlineCallbacks: (() => void)[] = [];
  const offlineCallbacks: (() => void)[] = [];

  // Update network status
  const updateNetworkStatus = () => {
    const online = navigator.onLine;
    isOnline.value = online;
    isOffline.value = !online;
    
    networkStatus.value = {
      isOnline: online,
      isOffline: !online,
      ...getConnectionInfo()
    };
  };

  // Get additional connection information (if available)
  const getConnectionInfo = () => {
    const connection = (navigator as any).connection || 
                     (navigator as any).mozConnection || 
                     (navigator as any).webkitConnection;
    
    if (connection) {
      return {
        effectiveType: connection.effectiveType,
        downlink: connection.downlink,
        rtt: connection.rtt
      };
    }
    
    return {};
  };

  // Event handlers
  const handleOnline = () => {
    updateNetworkStatus();
    onlineCallbacks.forEach(callback => {
      try {
        callback();
      } catch (error) {
        console.error('Error in online callback:', error);
      }
    });
  };

  const handleOffline = () => {
    updateNetworkStatus();
    offlineCallbacks.forEach(callback => {
      try {
        callback();
      } catch (error) {
        console.error('Error in offline callback:', error);
      }
    });
  };

  // Connection change handler (for mobile networks)
  const handleConnectionChange = () => {
    updateNetworkStatus();
  };

  // Active connection check
  const checkConnection = async (): Promise<boolean> => {
    if (!navigator.onLine) {
      return false;
    }

    try {
      // Try to fetch a small resource to verify actual connectivity
      const response = await fetch('/favicon.ico', {
        method: 'HEAD',
        cache: 'no-cache',
        signal: AbortSignal.timeout(5000) // 5 second timeout
      });
      
      return response.ok;
    } catch (error) {
      console.warn('Connection check failed:', error);
      return false;
    }
  };

  // Register callback for online events
  const onOnline = (callback: () => void) => {
    onlineCallbacks.push(callback);
  };

  // Register callback for offline events
  const onOffline = (callback: () => void) => {
    offlineCallbacks.push(callback);
  };

  // Setup event listeners
  onMounted(() => {
    // Basic online/offline events
    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);
    
    // Connection change events (mobile networks)
    const connection = (navigator as any).connection || 
                      (navigator as any).mozConnection || 
                      (navigator as any).webkitConnection;
    
    if (connection) {
      connection.addEventListener('change', handleConnectionChange);
    }
    
    // Initial status update
    updateNetworkStatus();
  });

  // Cleanup event listeners
  onUnmounted(() => {
    window.removeEventListener('online', handleOnline);
    window.removeEventListener('offline', handleOffline);
    
    const connection = (navigator as any).connection || 
                      (navigator as any).mozConnection || 
                      (navigator as any).webkitConnection;
    
    if (connection) {
      connection.removeEventListener('change', handleConnectionChange);
    }
    
    // Clear callbacks
    onlineCallbacks.length = 0;
    offlineCallbacks.length = 0;
  });

  return {
    isOnline,
    isOffline,
    networkStatus,
    checkConnection,
    onOnline,
    onOffline
  };
}