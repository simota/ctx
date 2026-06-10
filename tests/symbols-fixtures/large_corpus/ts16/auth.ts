import { type User } from "./user";

/** Get the active session for a user. */
export function getSession16(userId: string): Session16 | null { return null; }

export interface Session16 {
  token: string;
  user: string;
}

export class Symbol16 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session16 | null { return null; }

function privateImpl() {}
