// Mix API types and fetch wrappers — mirrors /api/mix endpoints.
// See api.ts for the shared ApiCallError + postJSON / getJSON patterns.
import { ApiCallError } from './api';

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

export interface MixBudget {
  plan?: string;
  limit: number;
}

// Summary row returned by GET /api/mix (list).
export interface MixSummary {
  id: string;
  name: string;
  goal: string;
  created: string; // RFC3339
  file_count: number;
}

// Full mix returned by GET /api/mix/<id> and POST /api/mix.
export interface Mix {
  schema_version: number;
  id: string;
  name: string;
  goal: string;
  created: string; // RFC3339
  files: string[];
  budget: MixBudget;
}

export interface MixListResponse {
  mixes: MixSummary[];
}

export interface SaveMixInput {
  name: string;
  goal: string;
  files: string[];
  budget: MixBudget;
}

// ---------------------------------------------------------------------------
// Validation limits (mirrored from backend contract).
// ---------------------------------------------------------------------------
export const MIX_NAME_MAX = 128;
export const MIX_GOAL_MAX = 1024;
export const MIX_FILES_MAX = 500;

// ---------------------------------------------------------------------------
// fetch helpers (mirrors api.ts getJSON / postJSON but scoped here to keep
// api.ts focused on the core Sherpa contract)
// ---------------------------------------------------------------------------

async function getJSON<T>(path: string): Promise<T> {
  const res = await fetch(path, { headers: { Accept: 'application/json' } });
  const text = await res.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiCallError(res.status, 'parse_error', `Invalid JSON from ${path}`);
    }
  }
  if (!res.ok) {
    const err = (body as { error?: { code?: string; message?: string } } | null)?.error;
    throw new ApiCallError(res.status, err?.code ?? 'http_error', err?.message ?? res.statusText);
  }
  return body as T;
}

async function postJSON<T>(path: string, req: unknown): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
    body: JSON.stringify(req),
  });
  const text = await res.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiCallError(res.status, 'parse_error', `Invalid JSON from ${path}`);
    }
  }
  if (!res.ok) {
    const err = (body as { error?: { code?: string; message?: string } } | null)?.error;
    throw new ApiCallError(res.status, err?.code ?? 'http_error', err?.message ?? res.statusText);
  }
  return body as T;
}

// ---------------------------------------------------------------------------
// API functions
// ---------------------------------------------------------------------------

export async function apiListMixes(): Promise<MixListResponse> {
  return getJSON<MixListResponse>('/api/mix');
}

export async function apiLoadMix(id: string): Promise<Mix> {
  return getJSON<Mix>(`/api/mix/${encodeURIComponent(id)}`);
}

export async function apiSaveMix(input: SaveMixInput): Promise<Mix> {
  return postJSON<Mix>('/api/mix', input);
}

export async function apiDeleteMix(id: string): Promise<void> {
  const url = `/api/mix/${encodeURIComponent(id)}`;
  const res = await fetch(url, {
    method: 'DELETE',
    headers: { Accept: 'application/json' },
  });
  if (res.status === 204) return;
  const text = await res.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiCallError(res.status, 'parse_error', `Invalid JSON from ${url}`);
    }
  }
  if (!res.ok) {
    const err = (body as { error?: { code?: string; message?: string } } | null)?.error;
    throw new ApiCallError(res.status, err?.code ?? 'http_error', err?.message ?? res.statusText);
  }
}
