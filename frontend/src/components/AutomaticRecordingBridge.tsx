'use client';
import { useEffect } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useTranscripts } from '@/contexts/TranscriptContext';

// Always mounted under the global providers, including when the window is hidden.
export function AutomaticRecordingBridge({ ready }: { ready: boolean }) {
  const { clearTranscripts, setMeetingTitle } = useTranscripts();
  useEffect(() => {
    let disposed = false;
    const listeners: UnlistenFn[] = [];
    let lastError: string | null = null;
    const setup = async () => {
      try {
        const prepare = await listen<string>('automatic-recording-prepare', async ({ payload }) => {
          clearTranscripts();
          setMeetingTitle('Automatic meeting');
          await invoke('automatic_recording_prepared', { request: payload });
        });
        if (disposed) { prepare(); return; }
        listeners.push(prepare);
        const status = await listen<{ error: string | null }>('automatic-recording-status', ({ payload }) => {
          if (payload.error && payload.error !== lastError) toast.error(payload.error);
          lastError = payload.error;
        });
        if (disposed) { status(); return; }
        listeners.push(status);
        await invoke('automatic_recording_frontend_ready', { ready });
        if (disposed) await invoke('automatic_recording_frontend_ready', { ready: false });
      } catch (error) { console.error('Automatic recording bridge failed:', error); }
    };
    void setup();
    return () => {
      disposed = true;
      listeners.forEach(unlisten => unlisten());
      void invoke('automatic_recording_frontend_ready', { ready: false });
    };
  }, [ready, clearTranscripts, setMeetingTitle]);
  return null;
}
