/**
 * Mock types for Alloy UI prototype.
 * These represent structures similar to what matrix-sdk-ui would provide.
 * Real types will be generated in apps/alloy/ui/src/lib/generated/ by another agent.
 */

export interface TimelineEvent {
  id: string;
  sender: string;
  senderAvatar?: string;
  timestamp: number;
  kind: "text" | "code" | "image" | "reply" | "day-divider";
  body: string;
  replyTo?: {
    id: string;
    sender: string;
    snippet: string;
  };
  codeLang?: string;
  imageUrl?: string;
  imageAspectRatio?: number; // width / height
  isUnread?: boolean;
  isLoading?: boolean;
}

export interface Room {
  id: string;
  name: string;
  topic?: string;
  unreadCount: number;
  lastMessage: string;
  lastMessageTimestamp: number;
  avatar?: string;
  isDirect: boolean;
}

export interface Space {
  id: string;
  name: string;
  icon?: string;
  rooms: Room[];
  isCollapsed?: boolean;
}

export interface Member {
  id: string;
  displayName: string;
  avatar?: string;
  status: "online" | "idle" | "offline";
  isYou?: boolean;
}
