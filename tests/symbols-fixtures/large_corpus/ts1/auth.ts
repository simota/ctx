import { type User } from "./user";

/** Get the active session for a user. */
export function getSession1(userId: string): Session1 | null { return null; }

export interface Session1 {
  token: string;
  user: string;
}

export class Symbol1 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session1 | null { return null; }

function privateImpl() {}
