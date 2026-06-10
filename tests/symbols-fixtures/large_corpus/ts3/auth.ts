import { type User } from "./user";

/** Get the active session for a user. */
export function getSession3(userId: string): Session3 | null { return null; }

export interface Session3 {
  token: string;
  user: string;
}

export class Symbol3 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session3 | null { return null; }

function privateImpl() {}
