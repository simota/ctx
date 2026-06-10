import { type User } from "./user";

/** Get the active session for a user. */
export function getSession14(userId: string): Session14 | null { return null; }

export interface Session14 {
  token: string;
  user: string;
}

export class Symbol14 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session14 | null { return null; }

function privateImpl() {}
