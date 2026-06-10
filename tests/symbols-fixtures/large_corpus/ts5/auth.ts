import { type User } from "./user";

/** Get the active session for a user. */
export function getSession5(userId: string): Session5 | null { return null; }

export interface Session5 {
  token: string;
  user: string;
}

export class Symbol5 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session5 | null { return null; }

function privateImpl() {}
