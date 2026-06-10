import { type User } from "./user";

/** Get the active session for a user. */
export function getSession19(userId: string): Session19 | null { return null; }

export interface Session19 {
  token: string;
  user: string;
}

export class Symbol19 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session19 | null { return null; }

function privateImpl() {}
