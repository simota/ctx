// TypeScript fixture: generics, interfaces, type aliases, class methods,
// nested functions, function overloads.
import { type User } from "./user";

export interface Repository<T, K = string> {
  find(id: K): Promise<T | null>;
  save(entity: T): Promise<void>;
}

export type Result<T> =
  | { ok: true; value: T }
  | { ok: false; error: string };

export function identity<T>(value: T): T {
  function clamp(): T {
    return value;
  }
  return clamp();
}

export class UserRepository implements Repository<User> {
  async find(id: string): Promise<User | null> {
    return null;
  }
  async save(entity: User): Promise<void> {}
}

interface Internal {
  flag: boolean;
}

type Alias = string | number;

function helper(): void {}
