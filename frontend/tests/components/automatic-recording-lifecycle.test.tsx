import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
const originals = {
  core: { ...await import('@tauri-apps/api/core') }, event: { ...await import('@tauri-apps/api/event') },
  navigation: { ...await import('next/navigation') }, toast: { ...await import('sonner') },
  state: { ...await import('../../src/contexts/RecordingStateContext') }, sidebar: { ...await import('../../src/components/Sidebar/SidebarProvider') },
  recording: { ...await import('../../src/services/recordingService') }, transcript: { ...await import('../../src/services/transcriptService') },
  indexed: { ...await import('../../src/services/indexedDBService') }, storage: { ...await import('../../src/services/storageService') },
  analytics: { ...await import('../../src/lib/analytics') }, language: { ...await import('../../src/lib/summary-language-preferences') },
  store: { ...await import('@tauri-apps/plugin-store') },
};
const callbacks = new Map<string, Set<(event: any) => unknown>>();
const listen = async (name: string, callback: (event: any) => unknown) => {
  if (!callbacks.has(name)) callbacks.set(name,new Set());
  callbacks.get(name)!.add(callback);
  return () => { callbacks.get(name)?.delete(callback); };
};
const emit = async (name: string, payload: unknown) => {
  await Promise.all([...callbacks.get(name) ?? []].map(callback => callback({ payload })));
};
let busy = false;
const completion = mock(async (command: string) => {
  if (command === 'automatic_recording_claim_preparation') return !busy;
  if (command === 'start_recording' && busy) throw new Error('The previous recording is still being saved');
  if (command === 'complete_recording_postprocessing') busy = false;
  if (command === 'get_meeting_folder_path') return '/old';
});
mock.module('@tauri-apps/api/core', () => ({ ...originals.core, invoke: completion }));
mock.module('@tauri-apps/api/event', () => ({ ...originals.event, listen }));
const push = mock(() => {});
const router = { push };
mock.module('next/navigation', () => ({ ...originals.navigation, useRouter: () => router }));
const notify = mock(() => {});
mock.module('sonner', () => ({ ...originals.toast, toast: { error: notify, warning: notify, success: notify } }));
const setStatus = mock((_status: string, _message?: string) => {});
mock.module('../../src/contexts/RecordingStateContext', () => ({ ...originals.state, useRecordingState: () => ({ isRecording: false, status: 'idle', setStatus }) }));
const sidebar = { refetchMeetings: async () => {}, setCurrentMeeting() {}, setMeetings() {}, meetings: [], setIsMeetingActive() {} };
mock.module('../../src/components/Sidebar/SidebarProvider', () => ({ ...originals.sidebar, useSidebar: () => sidebar }));
mock.module('../../src/services/recordingService', () => ({ recordingService: {
  onRecordingStarted: (cb: () => unknown) => listen('recording-started',cb),
  onRecordingStopped: (cb: () => unknown) => listen('recording-stopped',cb),
  getRecordingMeetingName: async () => 'Meeting',
} }));
mock.module('../../src/services/transcriptService', () => ({ transcriptService: {
  onTranscriptUpdate: (cb: () => unknown) => listen('transcript-update',cb),
  getTranscriptionStatus: async () => ({ is_processing:false,chunks_in_queue:0,last_activity_ms:0 }),
} }));
mock.module('../../src/services/indexedDBService', () => ({ indexedDBService: {
  init: async () => {}, saveMeetingMetadata: async () => {}, getMeetingMetadata: async () => null,
  markMeetingSaved: async () => {}, saveTranscripts: async () => {},
} }));
let saveEntered: () => void;
let finishSave: () => void;
let saveBlocked: Promise<void>;
let saveFailure = false;
const saved: Array<{ title: string; transcripts: any[] }> = [];
mock.module('../../src/services/storageService', () => ({ storageService: {
  saveMeeting: async (title: string, transcripts: any[]) => {
    saved.push({ title,transcripts }); saveEntered(); await saveBlocked; if (saveFailure) throw new Error('disk full'); return { meeting_id:`saved-${saved.length}` };
  },
  getMeeting: async () => ({ title:'Saved meeting' }),
} }));
mock.module('../../src/lib/analytics', () => ({ default: {
  trackPageView() {}, getMeetingsCountToday: async () => 1, trackMeetingCompleted: async () => {}, updateMeetingCount: async () => {},
} }));
mock.module('../../src/lib/summary-language-preferences', () => ({ ...originals.language, applyPinnedSummaryLanguageToMeeting: async () => true }));
mock.module('@tauri-apps/plugin-store', () => ({ ...originals.store, Store: { load: async () => ({ get:async () => 2 }) } }));
const { TranscriptProvider, useTranscripts } = await import('../../src/contexts/TranscriptContext');
const { AutomaticRecordingBridge } = await import('../../src/components/AutomaticRecordingBridge');
const { RecordingPostProcessingProvider } = await import('../../src/contexts/RecordingPostProcessingProvider');
let transcripts: ReturnType<typeof useTranscripts>;
function Probe() { transcripts = useTranscripts(); return <output>{transcripts.transcripts.map(t => t.text).join('|')}</output>; }
let renderer: ReactTestRenderer | undefined;
const realSetTimeout = globalThis.setTimeout;
const originalWindow = globalThis.window;
const originalStorage = globalThis.sessionStorage;
let navigate: (() => void) | undefined;
let entered: Promise<void>;
const advance = async () => { await act(async () => { await new Promise(resolve => realSetTimeout(resolve,5)); }); };
function newSave() {
  entered = new Promise(resolve => { saveEntered = resolve; });
  saveBlocked = new Promise(resolve => { finishSave = resolve; });
}
beforeEach(async () => {
  callbacks.clear(); saved.length=0; busy=false; saveFailure=false; completion.mockClear(); setStatus.mockClear(); navigate=undefined; newSave();
  globalThis.window = {} as Window & typeof globalThis;
  const values = new Map<string,string>();
  globalThis.sessionStorage = { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string,value: string) => { values.set(key,value); }, removeItem: (key: string) => { values.delete(key); } } as Storage;
  globalThis.setTimeout = ((callback: () => void,ms: number) => {
    if (ms === 2000) { navigate = callback; return 99999; }
    return realSetTimeout(callback,0);
  }) as typeof setTimeout;
  await act(async () => { renderer = create(<TranscriptProvider><RecordingPostProcessingProvider><AutomaticRecordingBridge ready /><Probe /></RecordingPostProcessingProvider></TranscriptProvider>); });
});
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer=undefined; globalThis.setTimeout=realSetTimeout; globalThis.window=originalWindow; globalThis.sessionStorage=originalStorage;
});
afterAll(() => {
  const modules = [ ['@tauri-apps/api/core',originals.core],['@tauri-apps/api/event',originals.event],['next/navigation',originals.navigation],['sonner',originals.toast],
    ['../../src/contexts/RecordingStateContext',originals.state],['../../src/components/Sidebar/SidebarProvider',originals.sidebar],
    ['../../src/services/recordingService',originals.recording],['../../src/services/transcriptService',originals.transcript],
    ['../../src/services/indexedDBService',originals.indexed],['../../src/services/storageService',originals.storage],
    ['../../src/lib/analytics',originals.analytics],['../../src/lib/summary-language-preferences',originals.language],['@tauri-apps/plugin-store',originals.store] ] as const;
  modules.forEach(([name,value]) => mock.module(name,() => value));
});
async function start(text: string) {
  await act(async () => { await emit('recording-started',{}); });
  await act(async () => { transcripts.addTranscript({ text,timestamp:'00:01',sequence_id:1,is_partial:false } as any); });
}
async function stop(id: number) {
  busy=true;
  await act(async () => { await emit('recording-stopped',{ session_id:id,folder_path:`/session-${id}`,meeting_name:`Meeting ${id}` }); await emit('recording-stop-complete',true); });
  await entered;
}
test('global providers retain the old transcript through delayed save and cleanup before preparing the next call', async () => {
  await start('first call'); await stop(1);
  await act(async () => { await emit('automatic-recording-prepare','next-call'); });
  expect(transcripts.transcriptsRef.current.map(t => t.text)).toEqual(['first call']);
  expect(busy).toBe(true);
  await act(async () => { finishSave(); }); await advance();
  expect(navigate).toBeDefined();
  expect(busy).toBe(true);
  await act(async () => { navigate!(); }); await advance();
  expect(busy).toBe(false);
  await act(async () => { await emit('automatic-recording-prepare','next-call-valid'); });
  expect(transcripts.transcriptsRef.current).toEqual([]);
  newSave(); await start('second call'); await stop(2);
  expect(saved.map(s => s.transcripts.map(t => t.text))).toEqual([['first call'],['second call']]);
  await act(async () => { finishSave(); }); await advance();
  await act(async () => { navigate!(); }); await advance();
});
test('a prior save cannot clear or set idle on a replacement manual session', async () => {
  await start('old call'); await stop(1);
  await act(async () => { finishSave(); }); await advance();
  await act(async () => { transcripts.clearTranscripts(); });
  await start('replacement manual call');
  setStatus.mockClear();
  await act(async () => { navigate!(); }); await advance();
  expect(transcripts.transcriptsRef.current.map(t => t.text)).toEqual(['replacement manual call']);
  expect(setStatus.mock.calls.some(call => call[0] === 'idle')).toBe(false);
});

test('failed prior save preserves transcripts and keeps automatic preparation blocked', async () => {
  saveFailure=true;
  await start('unsaved call'); await stop(1);
  await act(async () => { finishSave(); }); await advance();
  expect(busy).toBe(true);
  await act(async () => { await emit('automatic-recording-prepare','blocked-after-error'); });
  expect(transcripts.transcriptsRef.current.map(t => t.text)).toEqual(['unsaved call']);
  expect(setStatus.mock.calls.some(call => call[0] === 'error')).toBe(true);
  saveFailure=false;
  await act(async () => { await emit('retry-recording-postprocessing',true); });
  await advance();
  expect(busy).toBe(true);
  expect(navigate).toBeDefined();
  await act(async () => { navigate!(); }); await advance();
  expect(busy).toBe(false);
  expect(transcripts.transcriptsRef.current).toEqual([]);
});

test('a manual replacement start is rejected throughout the prior providers save and cleanup', async () => {
  await start('recording A'); await stop(1);
  await expect(completion('start_recording')).rejects.toThrow('previous recording is still being saved');
  expect(saved.length).toBe(1);
  expect(transcripts.transcriptsRef.current.map(t => t.text)).toEqual(['recording A']);
  await act(async () => { finishSave(); }); await advance();
  await expect(completion('start_recording')).rejects.toThrow('previous recording is still being saved');
  await act(async () => { navigate!(); }); await advance();
  await expect(completion('start_recording')).resolves.toBeUndefined();
});
