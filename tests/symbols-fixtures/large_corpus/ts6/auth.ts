import { type User } from "./user";

/** Get the active session for a user. */
export function getSession6(userId: string): Session6 | null { return null; }

export interface Session6 {
  token: string;
  user: string;
}

export class Symbol6 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session6 | null { return null; }

function privateImpl() {}
