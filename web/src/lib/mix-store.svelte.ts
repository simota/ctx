// Reactive mix store — wraps the /api/mix endpoints with $state so any
// component that reads mixStore.list re-renders when the list changes.
//
// Pattern mirrors ReplayPanel's local state but hoisted to module level so
// MixdownsPanel and BounceDialog share the same list without prop-drilling.

import {
  apiListMixes,
  apiLoadMix,
  apiSaveMix,
  apiDeleteMix,
  type MixSummary,
  type Mix,
  type SaveMixInput,
} from './mix-api';

interface MixStoreState {
  list: MixSummary[];
  loading: boolean;
  error: string | null;
}

export const mixStore = $state<MixStoreState>({
  list: [],
  loading: false,
  error: null,
});

export async function refreshMixList(): Promise<void> {
  mixStore.loading = true;
  mixStore.error = null;
  try {
    const r = await apiListMixes();
    mixStore.list = r.mixes;
  } catch (e: unknown) {
    mixStore.error = e instanceof Error ? e.message : String(e);
  } finally {
    mixStore.loading = false;
  }
}

export async function saveMix(input: SaveMixInput): Promise<Mix> {
  const mix = await apiSaveMix(input);
  // Optimistically prepend; refreshMixList will fix ordering if needed.
  mixStore.list = [
    {
      id: mix.id,
      name: mix.name,
      goal: mix.goal,
      created: mix.created,
      file_count: mix.files.length,
    },
    ...mixStore.list,
  ];
  return mix;
}

export async function deleteMix(id: string): Promise<void> {
  await apiDeleteMix(id);
  mixStore.list = mixStore.list.filter((m) => m.id !== id);
}

export async function loadMix(id: string): Promise<Mix> {
  return apiLoadMix(id);
}
