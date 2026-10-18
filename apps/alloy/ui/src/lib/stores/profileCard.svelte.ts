export interface ProfileCardAnchor {
  x: number;
  y: number;
}

interface ProfileCardState {
  isOpen: boolean;
  userId: string | null;
  roomId: string | null;
  at: ProfileCardAnchor | null;
}

const CLOSED: ProfileCardState = { isOpen: false, userId: null, roomId: null, at: null };

/** Which user's profile card is open, and where it is anchored on screen. */
export class ProfileCardStore {
  private card = $state<ProfileCardState>({ ...CLOSED });

  get isOpen(): boolean {
    return this.card.isOpen;
  }

  get userId(): string | null {
    return this.card.userId;
  }

  get roomId(): string | null {
    return this.card.roomId;
  }

  get at(): ProfileCardAnchor | null {
    return this.card.at;
  }

  /** Opens the card for a user, replacing any card already open. */
  open(userId: string, roomId: string | null, at: ProfileCardAnchor): void {
    this.card = { isOpen: true, userId, roomId, at: { x: at.x, y: at.y } };
  }

  close(): void {
    this.card = { ...CLOSED };
  }
}

export const profileCard = new ProfileCardStore();
