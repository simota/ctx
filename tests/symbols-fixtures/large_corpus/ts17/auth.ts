import { type User } from "./user";

/** Get the active session for a user. */
export function getSession17(userId: string): Session17 | null { return null; }

export interface Session17 {
  token: string;
  user: string;
}

export class Symbol17 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session17 | null { return null; }

function privateImpl() {}
