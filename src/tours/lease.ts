export const LEASE_KEY = 'tmgr:tours:lease';
export const LEASE_CHANNEL = 'tmgr-tours';
export const LEASE_TTL_MS = 30_000;

interface KeyValueStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

interface Channel {
  postMessage(data: unknown): void;
  onmessage: ((event: { data: unknown }) => void) | null;
  close?(): void;
}

export interface LeaseDeps {
  storage: KeyValueStorage;
  channel?: Channel | null;
  now: () => number;
  ownerId: string;
  ttlMs?: number;
}

export interface TourLease {
  acquire(): boolean;
  renew(): boolean;
  release(): void;
  holds(): boolean;
  broadcast(message: Record<string, unknown>): void;
  onMessage(fn: (message: Record<string, any>) => void): void;
}

interface Stored {
  owner: string;
  expires: number;
}

export function createLease(deps: LeaseDeps): TourLease {
  const ttl = deps.ttlMs ?? LEASE_TTL_MS;

  const read = (): Stored | null => {
    try {
      const raw = deps.storage.getItem(LEASE_KEY);
      if (!raw) return null;
      const parsed = JSON.parse(raw);
      return typeof parsed?.owner === 'string' && typeof parsed.expires === 'number'
        ? parsed
        : null;
    } catch {
      return null;
    }
  };

  const write = () => {
    try {
      deps.storage.setItem(
        LEASE_KEY,
        JSON.stringify({ owner: deps.ownerId, expires: deps.now() + ttl }),
      );
    } catch {
      return false;
    }
    return read()?.owner === deps.ownerId;
  };

  const holds = () => {
    const current = read();
    return !!current && current.owner === deps.ownerId && current.expires > deps.now();
  };

  const acquire = () => {
    const current = read();
    if (current && current.owner !== deps.ownerId && current.expires > deps.now()) return false;
    return write();
  };

  return {
    acquire,
    renew: () => (holds() ? write() : false),
    release() {
      if (read()?.owner === deps.ownerId) {
        try {
          deps.storage.removeItem(LEASE_KEY);
        } catch {
          return;
        }
      }
    },
    holds,
    broadcast(message) {
      try {
        deps.channel?.postMessage(message);
      } catch {
        return;
      }
    },
    onMessage(fn) {
      if (!deps.channel) return;
      deps.channel.onmessage = (event) => {
        if (event.data && typeof event.data === 'object') fn(event.data as Record<string, any>);
      };
    },
  };
}

export function createBrowserLease(): TourLease {
  let channel: Channel | null = null;
  try {
    channel = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel(LEASE_CHANNEL);
  } catch {
    channel = null;
  }
  const ownerId =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  return createLease({ storage: localStorage, channel, now: Date.now, ownerId });
}
