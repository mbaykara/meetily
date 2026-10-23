use anyhow::Result;
use log::{error, info, warn};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;

use super::audio_processing::create_meeting_folder;
use super::incremental_saver::IncrementalAudioSaver;
use super::recording_state::AudioChunk;

pub use super::transcript_files::{DeviceInfo, MeetingMetadata, TranscriptSegment};

/// New recording saver using incremental saving strategy
pub struct RecordingSaver {
    incremental_saver: Option<Arc<AsyncMutex<IncrementalAudioSaver>>>,
    finalized_audio_path: Option<PathBuf>,
    meeting_folder: Option<PathBuf>,
    base_folder: Option<PathBuf>,
    meeting_name: Option<String>,
    metadata: Option<MeetingMetadata>,
    transcript_sink: super::transcript_files::TranscriptSink,
    transcript_only: bool,
    is_saving: Arc<Mutex<bool>>,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            incremental_saver: None,
            finalized_audio_path: None,
            meeting_folder: None,
            base_folder: None,
            meeting_name: None,
            metadata: None,
            transcript_sink: super::transcript_files::TranscriptSink::new(
                None,
                "Transcript".into(),
            ),
            transcript_only: false,
            is_saving: Arc::new(Mutex::new(false)),
        }
    }

    /// Set the meeting name for this recording session
    pub fn set_meeting_name(&mut self, name: Option<String>) {
        self.meeting_name = name;
    }

    pub fn set_recording_folder(&mut self, folder: PathBuf) {
        self.base_folder = Some(folder);
    }
    pub fn set_transcript_only_session(&mut self) {
        self.transcript_only = true;
    }
    pub fn transcript_sink(&self) -> super::transcript_files::TranscriptSink {
        self.transcript_sink.clone()
    }

    /// Set device information in metadata
    pub fn set_device_info(&mut self, mic_name: Option<String>, sys_name: Option<String>) {
        if let Some(ref mut metadata) = self.metadata {
            metadata.devices.microphone = mic_name;
            metadata.devices.system_audio = sys_name;

            // Write updated metadata to disk if folder exists
            if let Some(folder) = &self.meeting_folder {
                let metadata_clone = metadata.clone();
                if let Err(_) = self.write_metadata(folder, &metadata_clone) {
                    warn!("Failed to update metadata with device info");
                }
            }
        }
    }

    /// Add or update a structured transcript segment (upserts based on sequence_id)
    /// Also saves incrementally to disk
    pub fn add_transcript_segment(&self, segment: TranscriptSegment) {
        if let Err(error) = self.transcript_sink.upsert(segment) {
            warn!("Incremental transcript persistence failed: {error}");
        }
    }

    /// Legacy method for backward compatibility - converts text to basic segment
    pub fn add_transcript_chunk(&self, text: String) {
        let segment = TranscriptSegment {
            id: format!("seg_{}", chrono::Utc::now().timestamp_millis()),
            text,
            audio_start_time: 0.0,
            audio_end_time: 0.0,
            duration: 0.0,
            display_time: "[00:00]".to_string(),
            confidence: 1.0,
            sequence_id: 0,
        };
        self.add_transcript_segment(segment);
    }

    /// Start accumulation with optional incremental saving
    ///
    /// # Arguments
    /// * `auto_save` - If true, creates checkpoints and enables saving. If false, audio chunks are discarded.
    pub fn start_accumulation(
        &mut self,
        auto_save: bool,
        mut receiver: mpsc::UnboundedReceiver<AudioChunk>,
    ) -> Result<()> {
        let auto_save = super::transcript_files::retain_audio(self.transcript_only, auto_save);
        if auto_save {
            info!("Initializing incremental audio saver for recording (auto-save ENABLED)");
        } else {
            info!(
                "Starting recording without audio saving (auto-save DISABLED - transcripts only)"
            );
        }

        let name = self
            .meeting_name
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Meeting name was not initialized"))?;
        self.initialize_meeting_folder(&name, auto_save)?;
        *self
            .is_saving
            .lock()
            .map_err(|_| anyhow::anyhow!("Recording saver lock unavailable"))? = true;

        // Start accumulation task
        let is_saving_clone = self.is_saving.clone();
        let incremental_saver_arc = self.incremental_saver.clone();
        let save_audio = auto_save;

        tokio::spawn(async move {
            info!(
                "Recording saver accumulation task started (save_audio: {})",
                save_audio
            );

            while let Some(chunk) = receiver.recv().await {
                // Check if we should continue
                let should_continue = if let Ok(is_saving) = is_saving_clone.lock() {
                    *is_saving
                } else {
                    false
                };

                if !should_continue {
                    break;
                }

                // Only process audio chunks if auto_save is enabled
                if save_audio {
                    // Add chunk to incremental saver
                    if let Some(saver_arc) = &incremental_saver_arc {
                        let mut saver_guard = saver_arc.lock().await;
                        if saver_guard.add_chunk(chunk).is_err() {
                            error!("Failed to add chunk to incremental saver");
                        }
                    } else {
                        error!("Incremental saver not available while accumulating");
                    }
                } else {
                    // auto_save is false: discard audio chunk (no-op)
                    // Transcription already happened in the pipeline before this point
                }
            }

            info!("Recording saver accumulation task ended");
        });

        Ok(())
    }

    /// Initialize meeting folder structure and metadata
    ///
    /// # Arguments
    /// * `meeting_name` - Name of the meeting
    /// * `create_checkpoints` - Whether to create .checkpoints/ directory and IncrementalAudioSaver
    fn initialize_meeting_folder(
        &mut self,
        meeting_name: &str,
        create_checkpoints: bool,
    ) -> Result<()> {
        let base_folder = self
            .base_folder
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Recording folder was not configured"))?;
        super::storage_location::validate_directory(base_folder)?;

        // Create meeting folder structure (with or without .checkpoints/ subdirectory)
        let meeting_folder = if self.transcript_only {
            super::transcript_files::create_transcript_session_folder(base_folder, meeting_name)?
        } else {
            create_meeting_folder(base_folder, meeting_name, create_checkpoints)?
        };

        // Only initialize incremental saver if checkpoints are needed (auto_save is true)
        if create_checkpoints {
            let incremental_saver = IncrementalAudioSaver::new(meeting_folder.clone(), 48000)?;
            self.incremental_saver = Some(Arc::new(AsyncMutex::new(incremental_saver)));
            info!("Incremental audio saver initialized");
        } else {
            info!("⚠️  Skipped incremental audio saver (auto-save disabled)");
        }

        // Create initial metadata
        let metadata = MeetingMetadata {
            version: "1.0".to_string(),
            meeting_id: None, // Will be set by backend
            meeting_name: Some(meeting_name.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            duration_seconds: None,
            devices: DeviceInfo {
                microphone: None, // Could be enhanced to store actual device names
                system_audio: None,
            },
            audio_file: if create_checkpoints {
                "audio.mp4".to_string()
            } else {
                "".to_string()
            },
            transcript_file: "transcripts.json".to_string(),
            sample_rate: 48000,
            status: "recording".to_string(),
        };

        // Write initial metadata.json
        self.write_metadata(&meeting_folder, &metadata)?;

        self.transcript_sink = super::transcript_files::TranscriptSink::new(
            Some(meeting_folder.clone()),
            meeting_name.into(),
        );
        self.meeting_folder = Some(meeting_folder);
        self.metadata = Some(metadata);

        Ok(())
    }

    /// Write metadata.json to disk (atomic write with temp file)
    fn write_metadata(&self, folder: &PathBuf, metadata: &MeetingMetadata) -> Result<()> {
        super::transcript_files::write_metadata(folder, metadata)
    }

    // in frontend/src-tauri/src/audio/recording_saver.rs
    pub fn get_stats(&self) -> (usize, u32) {
        if let Some(ref saver) = self.incremental_saver {
            if let Ok(guard) = saver.try_lock() {
                (guard.get_checkpoint_count() as usize, 48000)
            } else {
                (0, 48000)
            }
        } else {
            (0, 48000)
        }
    }

    /// Stop and save using incremental saving approach
    ///
    /// # Arguments
    /// * `app` - Tauri app handle for emitting events
    /// * `recording_duration` - Actual recording duration in seconds (from RecordingState)
    pub async fn stop_and_save<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        recording_duration: Option<f64>,
    ) -> Result<Option<String>, String> {
        info!("Stopping recording saver");

        // Stop accumulation
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = false;
        }

        // Give time for final chunks
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let final_audio_path = self.finalize_storage(recording_duration).await?;

        // Emit save event with audio and transcript paths
        let save_event = serde_json::json!({
            "audio_file": final_audio_path.as_ref().map(|path|path.to_string_lossy().to_string()),
            "transcript_text_file": self.meeting_folder.as_ref().map(|folder|folder.join(format!("{}.txt",super::transcript_files::safe_basename(self.meeting_name.as_deref().unwrap_or("Transcript")))).to_string_lossy().to_string()),
            "transcript_file": self.meeting_folder.as_ref()
                .map(|f| f.join("transcripts.json").to_string_lossy().to_string()),
            "meeting_name": self.meeting_name,
            "meeting_folder": self.meeting_folder.as_ref()
                .map(|f| f.to_string_lossy().to_string())
        });

        if let Err(e) = app.emit("recording-saved", &save_event) {
            warn!("Failed to emit recording-saved event: {}", e);
        }

        // Keep the in-memory copy until the manager drops, including on errors.
        Ok(final_audio_path.map(|path| path.to_string_lossy().to_string()))
    }

    async fn finalize_storage(
        &mut self,
        recording_duration: Option<f64>,
    ) -> Result<Option<PathBuf>, String> {
        // Persist every final transcript before optional audio finalization.
        self.transcript_sink
            .write()
            .map_err(|error| format!("Failed to save transcripts: {error}"))?;
        let final_audio_path = if let Some(path) = &self.finalized_audio_path {
            Some(path.clone())
        } else if let Some(saver_arc) = &self.incremental_saver {
            let mut saver = saver_arc.lock().await;
            let path = saver
                .finalize()
                .await
                .map_err(|error| format!("Failed to finalize audio: {error}"))?;
            // Finalization removes checkpoints. Retain its successful result
            // before any retryable transcript/metadata write can fail.
            self.finalized_audio_path = Some(path.clone());
            Some(path)
        } else {
            None
        };
        let metadata = self
            .metadata
            .as_ref()
            .ok_or_else(|| "Recording metadata was not initialized".to_string())?;
        self.metadata = Some(
            self.transcript_sink
                .finalize(metadata, recording_duration)
                .map_err(|error| format!("Failed to finalize transcripts: {error}"))?,
        );

        Ok(final_audio_path)
    }

    /// Get the meeting folder path (for passing to backend)
    pub fn get_meeting_folder(&self) -> Option<&PathBuf> {
        self.meeting_folder.as_ref()
    }

    /// Get accumulated transcript segments (for reload sync)
    pub fn get_transcript_segments(&self) -> Vec<TranscriptSegment> {
        self.transcript_sink.snapshot().unwrap_or_default()
    }

    /// Get meeting name (for reload sync)
    pub fn get_meeting_name(&self) -> Option<String> {
        self.meeting_name.clone()
    }
}

impl Default for RecordingSaver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::recording_state::DeviceType;
    use super::*;

    #[tokio::test]
    async fn configured_root_is_snapshotted_for_manual_and_automatic_sessions() {
        for automatic in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let selected = root.path().join("selected");
            let later = root.path().join("later");
            let mut preference = selected.clone();
            let mut saver = RecordingSaver::new();
            saver.set_recording_folder(preference.clone());
            if automatic {
                saver.set_transcript_only_session();
            }
            saver.meeting_name = Some("Teams 2026-10-23_21-50-12".into());
            saver
                .initialize_meeting_folder("Teams 2026-10-23_21-50-12", false)
                .unwrap();
            preference = later.clone();
            assert_eq!(preference, later);
            saver.add_transcript_chunk("Stored in selected root".into());
            saver.finalize_storage(Some(1.0)).await.unwrap();
            let folder = saver.get_meeting_folder().unwrap();
            assert!(folder.starts_with(&selected));
            assert!(folder.join("transcripts.json").is_file());
            assert!(!later.exists());
        }
    }

    #[tokio::test]
    async fn manual_audio_metadata_retry_reuses_finalized_file() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("Manual session");
        std::fs::create_dir_all(folder.join(".checkpoints")).unwrap();
        let mut saver = RecordingSaver::new();
        let mut audio = IncrementalAudioSaver::new(folder.clone(), 48000).unwrap();
        audio
            .add_chunk(AudioChunk {
                data: vec![0.25; 24000],
                sample_rate: 48000,
                timestamp: 0.0,
                chunk_id: 1,
                device_type: DeviceType::Microphone,
            })
            .unwrap();
        saver.incremental_saver = Some(Arc::new(AsyncMutex::new(audio)));
        saver.meeting_folder = Some(folder.clone());
        saver.meeting_name = Some("Manual session".into());
        saver.transcript_sink = super::super::transcript_files::TranscriptSink::new(
            Some(folder.clone()),
            "Manual session".into(),
        );
        saver.metadata = Some(MeetingMetadata {
            version: "1.0".into(),
            meeting_id: None,
            meeting_name: Some("Manual session".into()),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            duration_seconds: None,
            devices: DeviceInfo {
                microphone: None,
                system_audio: None,
            },
            audio_file: "audio.mp4".into(),
            transcript_file: "transcripts.json".into(),
            sample_rate: 48000,
            status: "recording".into(),
        });
        saver.add_transcript_chunk("Final transcript survives retry".into());
        let metadata_path = folder.join("metadata.json");
        std::fs::create_dir(&metadata_path).unwrap();
        let first = saver.finalize_storage(Some(0.5)).await;
        assert!(first
            .unwrap_err()
            .contains("Failed to finalize transcripts"));
        assert!(folder.join("audio.mp4").is_file());
        assert!(!folder.join(".checkpoints").exists());
        let audio_before = std::fs::read(folder.join("audio.mp4")).unwrap();
        assert!(!audio_before.is_empty());
        std::fs::remove_dir(&metadata_path).unwrap();
        let path = saver.finalize_storage(Some(0.5)).await.unwrap().unwrap();
        assert_eq!(path, folder.join("audio.mp4"));
        assert_eq!(std::fs::read(&path).unwrap(), audio_before);
        let metadata: MeetingMetadata =
            serde_json::from_slice(&std::fs::read(metadata_path).unwrap()).unwrap();
        assert_eq!(metadata.status, "completed");
        assert_eq!(metadata.duration_seconds, Some(0.5));
        assert_eq!(saver.get_transcript_segments().len(), 1);
        assert!(std::fs::read_to_string(folder.join("Manual session.txt"))
            .unwrap()
            .contains("Final transcript survives retry"));
    }
}
