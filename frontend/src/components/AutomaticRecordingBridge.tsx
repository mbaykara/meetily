'use client';
import { useEffect } from 'react';
import { flushSync } from 'react-dom';
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
          try {
            // Claim the native reservation before touching shared session state.
            const claimed = await invoke<boolean>('automatic_recording_claim_preparation', { request: payload });
            if (!claimed || disposed) {
              if (claimed) await invoke('automatic_recording_cancel_preparation', { request: payload });
              return;
            }
            flushSync(() => {
              clearTranscripts();
              setMeetingTitle('Automatic meeting');
            });
            await invoke('automatic_recording_prepared', { request: payload });
          } catch (error) {
            await invoke('automatic_recording_cancel_preparation', { request: payload });
            toast.error(`Automatic recording preparation failed: ${String(error)}`);
          }
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
