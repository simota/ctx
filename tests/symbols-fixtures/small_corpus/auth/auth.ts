import { type User } from "./user";

/** Get the active session for a user. */
export function getSession(userId: string): Session | null { return null; }

export interface Session {
  token: string;
  user: string;
}

export class Symbol {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session | null { return null; }

function privateImpl() {}
