import { type User } from "./user";

/** Get the active session for a user. */
export function getSession11(userId: string): Session11 | null { return null; }

export interface Session11 {
  token: string;
  user: string;
}

export class Symbol11 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session11 | null { return null; }

function privateImpl() {}
