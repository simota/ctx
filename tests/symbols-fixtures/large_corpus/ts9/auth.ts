import { type User } from "./user";

/** Get the active session for a user. */
export function getSession9(userId: string): Session9 | null { return null; }

export interface Session9 {
  token: string;
  user: string;
}

export class Symbol9 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session9 | null { return null; }

function privateImpl() {}
