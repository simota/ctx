import { type User } from "./user";

/** Get the active session for a user. */
export function getSession15(userId: string): Session15 | null { return null; }

export interface Session15 {
  token: string;
  user: string;
}

export class Symbol15 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session15 | null { return null; }

function privateImpl() {}
