import { type User } from "./user";

/** Get the active session for a user. */
export function getSession12(userId: string): Session12 | null { return null; }

export interface Session12 {
  token: string;
  user: string;
}

export class Symbol12 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session12 | null { return null; }

function privateImpl() {}
