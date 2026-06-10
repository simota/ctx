import { type User } from "./user";

/** Get the active session for a user. */
export function getSession4(userId: string): Session4 | null { return null; }

export interface Session4 {
  token: string;
  user: string;
}

export class Symbol4 {
  constructor(public name: string, public kind: string) {}
  Render(): string { return `${this.name}(${this.kind})`; }
}

export function BuildIndex(root: string): Session4 | null { return null; }

function privateImpl() {}
