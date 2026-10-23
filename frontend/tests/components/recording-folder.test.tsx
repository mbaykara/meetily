import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
const originals = {
  core:{ ...await import('@tauri-apps/api/core') }, toast:{ ...await import('sonner') },
  config:{ ...await import('../../src/contexts/ConfigContext') }, state:{ ...await import('../../src/contexts/RecordingStateContext') },
  device:{ ...await import('../../src/components/DeviceSelection') }, automatic:{ ...await import('../../src/components/AutomaticRecordingSettings') },
  analytics:{ ...await import('../../src/lib/analytics') }, store:{ ...await import('@tauri-apps/plugin-store') },
};
let preferences: { save_folder:string;auto_save:boolean;file_format:string;preferred_mic_device:null;preferred_system_device:null };
let selection: string | null = '/chosen/calls';
let failPersistence=false;
let pickerError=false;
let recording=false;
const notify=mock(() => {});
const invoke=mock(async (command:string,args?: any) => {
  if (command === 'get_recording_preferences') return {...preferences};
  if (command === 'select_recording_folder') { if (pickerError) throw new Error('Folder is not writable'); return selection; }
  if (command === 'set_recording_preferences') { if (failPersistence) throw new Error('Disk full'); preferences={...args.preferences}; }
  if (command === 'open_recordings_folder') return;
});
mock.module('@tauri-apps/api/core',() => ({...originals.core,invoke}));
mock.module('sonner',() => ({...originals.toast,toast:{error:notify,success:notify}}));
mock.module('../../src/contexts/ConfigContext',() => ({...originals.config,useConfig:() => ({setSelectedDevices() {}})}));
mock.module('../../src/contexts/RecordingStateContext',() => ({...originals.state,useRecordingState:() => ({isRecording:recording})}));
mock.module('../../src/components/DeviceSelection',() => ({...originals.device,DeviceSelection:() => null}));
mock.module('../../src/components/AutomaticRecordingSettings',() => ({AutomaticRecordingSettings:() => null}));
mock.module('../../src/lib/analytics',() => ({default:{track:async () => {}}}));
mock.module('@tauri-apps/plugin-store',() => ({...originals.store,Store:{load:async () => ({get:async () => true})}}));
const { RecordingSettings } = await import('../../src/components/RecordingSettings');
let renderer:ReactTestRenderer|undefined;
const button=(label:string) => renderer!.root.findAllByType('button').find(node => node.props.children===label)!;
beforeEach(async () => {
  preferences={save_folder:'/original/calls',auto_save:false,file_format:'mp4',preferred_mic_device:null,preferred_system_device:null};
  selection='/chosen/calls'; failPersistence=false;pickerError=false;recording=false;invoke.mockClear();notify.mockClear();
});
afterEach(async () => { if (renderer) await act(async () => renderer!.unmount());renderer=undefined; });
afterAll(() => {
  mock.module('@tauri-apps/api/core',() => originals.core);mock.module('sonner',() => originals.toast);
  mock.module('../../src/contexts/ConfigContext',() => originals.config);mock.module('../../src/contexts/RecordingStateContext',() => originals.state);
  mock.module('../../src/components/DeviceSelection',() => originals.device);mock.module('../../src/components/AutomaticRecordingSettings',() => originals.automatic);
  mock.module('../../src/lib/analytics',() => originals.analytics);mock.module('@tauri-apps/plugin-store',() => originals.store);
});
async function mount() { await act(async () => { renderer=create(<RecordingSettings />); }); }
test('folder selection remains visible for transcript-only sessions and persists the selected root',async () => {
  await mount();expect(button('Choose Folder')).toBeDefined();expect(JSON.stringify(renderer!.toJSON())).toContain('/original/calls');
  await act(async () => { await button('Choose Folder').props.onClick(); });
  expect(preferences.save_folder).toBe('/chosen/calls');expect(preferences.auto_save).toBe(false);
  expect(JSON.stringify(renderer!.toJSON())).toContain('/chosen/calls');
  await act(async () => { await renderer!.root.findByProps({ 'aria-label':'Open calls folder' }).props.onClick(); });
  expect(invoke.mock.calls.some(call => call[0]==='open_recordings_folder')).toBe(true);
});
test('cancelling folder selection leaves preferences and UI unchanged',async () => {
  selection=null;await mount();await act(async () => { await button('Choose Folder').props.onClick(); });
  expect(preferences.save_folder).toBe('/original/calls');expect(invoke.mock.calls.some(call => call[0]==='set_recording_preferences')).toBe(false);
  expect(JSON.stringify(renderer!.toJSON())).toContain('/original/calls');
});
test('failed preference persistence retains the old folder and reports an error',async () => {
  failPersistence=true;await mount();await act(async () => { await button('Choose Folder').props.onClick(); });
  expect(preferences.save_folder).toBe('/original/calls');expect(JSON.stringify(renderer!.toJSON())).toContain('/original/calls');
  expect(JSON.stringify(renderer!.toJSON())).not.toContain('/chosen/calls');expect(notify.mock.calls.length).toBeGreaterThan(0);
});
test('an unusable picked folder is not persisted',async () => {
  pickerError=true;await mount();await act(async () => { await button('Choose Folder').props.onClick(); });
  expect(preferences.save_folder).toBe('/original/calls');expect(invoke.mock.calls.some(call => call[0]==='set_recording_preferences')).toBe(false);
});
test('the picker is disabled while a recording is active',async () => {
  recording=true;await mount();expect(button('Choose Folder').props.disabled).toBe(true);
  expect(JSON.stringify(renderer!.toJSON())).toContain('Stop the current recording');
});
