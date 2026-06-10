import { type User } from "./user";

/** Get the active session for a user. */
export function getSession8(userId: string): Session8 | null { return null; }

export interface Session8 {
  token: string;
  user: string;
}

export class Symbol8 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session8 | null { return null; }

function privateImpl() {}
