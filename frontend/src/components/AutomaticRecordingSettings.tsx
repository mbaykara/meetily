'use client';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Switch } from '@/components/ui/switch';
import { toast } from 'sonner';

type Preferences = { teams: boolean; zoom: boolean };
type Status = {
  preferences: Preferences; supported: boolean; accessibility: boolean;
  teams: string; zoom: string; error: string | null; waiting_for_save: boolean;
};
export function AutomaticRecordingSettings() {
  const [status, setStatus] = useState<Status | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<Status>('automatic-recording-status', e => { if (!disposed) setStatus(e.payload); });
        if (disposed) { unlisten(); return; }
        cleanup = unlisten;
        const current = await invoke<Status>('get_automatic_recording_status');
        if (!disposed) setStatus(current);
      } catch (e) { if (!disposed) setError(String(e)); }
    })();
    return () => { disposed = true; cleanup?.(); };
  }, []);
  const save = async (app: keyof Preferences, enabled: boolean) => {
    if (!status) return;
    setSaving(true);
    try {
      const preferences = { ...status.preferences, [app]: enabled };
      await invoke('set_automatic_recording_preferences', { preferences });
      setStatus(previous => previous ? { ...previous, preferences } : previous);
      setError(null);
    } catch (e) { setError(`Could not save automatic recording preferences: ${String(e)}`); }
    finally { setSaving(false); }
  };
  return <section className="space-y-4 border-t pt-6">
    <h4 className="text-base font-medium">Automatic recording</h4>
    <p className="text-sm text-gray-600">Automatically record Teams and Zoom calls. Stop after all selected calls have ended for ten seconds. Meetily must remain running.</p>
    {!status && !error && <p role="status">Loading automatic recording settings...</p>}
    {status && !status.supported && <p role="status">Automatic call detection is available on macOS only.</p>}
    {status?.supported && <>
      {(['teams', 'zoom'] as const).map(app => <div key={app} className="flex items-center justify-between p-4 border rounded-lg">
        <div><div className="font-medium">{app === 'teams' ? 'Microsoft Teams' : 'Zoom'}</div>
          <div className="text-sm text-gray-600">Detection: {status[app]}</div></div>
        <Switch aria-label={`Automatically record ${app === 'teams' ? 'Microsoft Teams' : 'Zoom'} calls`} checked={status.preferences[app]} disabled={saving} onCheckedChange={enabled => void save(app, enabled)} />
      </div>)}
      {(status.preferences.teams || status.preferences.zoom) && !status.accessibility && <div className="text-sm space-y-2">
        <p>Accessibility permission is required. Enable Meetily in macOS Privacy &amp; Security, then return here. Detection remains uncertain until access is available.</p>
        <button className="px-3 py-2 border rounded-md" onClick={() => void invoke('open_automatic_recording_accessibility').catch(e => toast.error(String(e)))}>Open Accessibility settings</button>
      </div>}
      <p className="text-sm text-gray-600">Unknown detection never stops a recording. Supported English and German controls depend on your app version. Manual recordings stay under your control. Turning these switches off leaves a current recording running.</p>
    </>}
    {status?.waiting_for_save && <div className="space-y-2 text-sm text-gray-600">
      <p role="status">Waiting for the previous recording to finish saving before starting another.</p>
      <button className="px-3 py-2 border rounded-md" onClick={() => void invoke('retry_recording_postprocessing').catch(e => toast.error(String(e)))}>Retry save</button>
    </div>}
    {(error || status?.error) && <p role="alert" className="text-sm text-red-700">{error || status?.error}</p>}
  </section>;
}
