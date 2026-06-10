import { type User } from "./user";

/** Get the active session for a user. */
export function getSession7(userId: string): Session7 | null { return null; }

export interface Session7 {
  token: string;
  user: string;
}

export class Symbol7 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session7 | null { return null; }

function privateImpl() {}
