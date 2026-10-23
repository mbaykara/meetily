import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvent = { ...await import('@tauri-apps/api/event') };
const originalToast = { ...await import('sonner') };
const originalTranscript = { ...await import('../../src/contexts/TranscriptContext') };
const handlers = new Map<string, (e: any) => unknown>();
const order: string[] = [];
let failSave = false;
let claimAllowed = true;
let supported = true;
const preferences = { teams: false, zoom: false };
const invoke = mock(async (command: string) => {
  order.push(command);
  if (command === 'automatic_recording_claim_preparation') return claimAllowed;
  if (command === 'get_automatic_recording_status') return { preferences, supported, accessibility: false, teams: 'unknown', zoom: 'unknown', error: null };
  if (command === 'set_automatic_recording_preferences' && failSave) throw new Error('disk full');
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
mock.module('@tauri-apps/api/event', () => ({ ...originalEvent, listen: async (name: string, callback: (e: any) => unknown) => { handlers.set(name, callback); return () => { handlers.delete(name); }; } }));
mock.module('sonner', () => ({ ...originalToast, toast: { error: mock(() => {}) } }));
const clearTranscripts = () => { order.push('clear'); };
let preparedTitle = '';
const setMeetingTitle = (title: string) => { preparedTitle = title; order.push('title'); };
mock.module('../../src/contexts/TranscriptContext', () => ({ ...originalTranscript, useTranscripts: () => ({ clearTranscripts, setMeetingTitle }) }));
const { AutomaticRecordingBridge } = await import('../../src/components/AutomaticRecordingBridge');
const { AutomaticRecordingSettings } = await import('../../src/components/AutomaticRecordingSettings');
let renderer: ReactTestRenderer | undefined;
beforeEach(() => { handlers.clear(); order.length = 0; invoke.mockClear(); failSave = false; supported = true; claimAllowed = true; });
afterEach(async () => { if (renderer) await act(async () => renderer!.unmount()); renderer = undefined; });
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@tauri-apps/api/event', () => originalEvent);
  mock.module('sonner', () => originalToast);
  mock.module('../../src/contexts/TranscriptContext', () => originalTranscript);
});
test('global bridge clears transcript state before acknowledging native preparation', async () => {
  await act(async () => { renderer = create(<AutomaticRecordingBridge ready />); });
  expect(handlers.has('automatic-recording-prepare')).toBe(true);
  order.length = 0;
  await act(async () => { await handlers.get('automatic-recording-prepare')!({ payload: { request: 'request-1', meeting_name: 'Teams 2026-10-23_21-50-12' } }); });
  expect(order).toEqual(['automatic_recording_claim_preparation', 'clear', 'title', 'automatic_recording_prepared']);
  expect(preparedTitle).toBe('Teams 2026-10-23_21-50-12');
  expect(invoke.mock.calls.some(call => call[0] === 'automatic_recording_frontend_ready')).toBe(true);
});
test('failed persistence leaves the switch disabled and exposes the error', async () => {
  await act(async () => { renderer = create(<AutomaticRecordingSettings />); });
  failSave = true;
  const control = renderer!.root.findAllByProps({ 'aria-label': 'Automatically record Microsoft Teams calls' }).find(node => typeof node.props.onCheckedChange === 'function')!;
  await act(async () => { await control.props.onCheckedChange(true); });
  expect(renderer!.root.findAllByProps({ 'aria-label': 'Automatically record Microsoft Teams calls' }).find(node => typeof node.props.onCheckedChange === 'function')!.props.checked).toBe(false);
  expect(JSON.stringify(renderer!.toJSON())).toContain('disk full');
});
test('unsupported platform offers no switches', async () => {
  supported = false;
  await act(async () => { renderer = create(<AutomaticRecordingSettings />); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('available on macOS only');
  expect(renderer!.root.findAllByProps({ role: 'switch' }).length).toBe(0);
});
test('missing accessibility permission is actionable after opt in', async () => {
  await act(async () => { renderer = create(<AutomaticRecordingSettings />); });
  await act(async () => { await renderer!.root.findAllByProps({ 'aria-label': 'Automatically record Microsoft Teams calls' }).find(node => typeof node.props.onCheckedChange === 'function')!.props.onCheckedChange(true); });
  expect(JSON.stringify(renderer!.toJSON())).toContain('Accessibility permission is required');
  expect(JSON.stringify(renderer!.toJSON())).toContain('Open Accessibility settings');
});

test('expired preparation after a replacement manual session cannot clear state', async () => {
  await act(async () => { renderer = create(<AutomaticRecordingBridge ready />); });
  claimAllowed = false;
  order.length = 0;
  await act(async () => { await handlers.get('automatic-recording-prepare')!({ payload: { request: 'expired-request', meeting_name: 'Teams 2026-10-23_21-50-12' } }); });
  expect(order).toEqual(['automatic_recording_claim_preparation']);
  expect(order).not.toContain('clear');
});

test('waiting save exposes an explicit retry action', async () => {
  await act(async () => { renderer = create(<AutomaticRecordingSettings />); });
  await act(async () => { handlers.get('automatic-recording-status')!({ payload: {
    preferences, supported:true, accessibility:true, teams:'inactive', zoom:'inactive', error:null, waiting_for_save:true,
  } }); });
  const button = renderer!.root.findAllByType('button').find(node => node.props.children === 'Retry save')!;
  await act(async () => { await button.props.onClick(); });
  expect(order).toContain('retry_recording_postprocessing');
});
