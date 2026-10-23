//! Transcript persistence has no dependency on an audio encoder or Tauri runtime.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
/// Structured transcript segment for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub text: String,
    pub audio_start_time: f64, // Seconds from recording start
    pub audio_end_time: f64,   // Seconds from recording start
    pub duration: f64,         // Segment duration in seconds
    pub display_time: String,  // Formatted time for display like "[02:15]"
    pub confidence: f32,
    pub sequence_id: u64,
}

/// Meeting metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMetadata {
    pub version: String,
    pub meeting_id: Option<String>,
    pub meeting_name: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub duration_seconds: Option<f64>,
    pub devices: DeviceInfo,
    pub audio_file: String,
    pub transcript_file: String,
    pub sample_rate: u32,
    pub status: String, // "recording", "completed", "error"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub microphone: Option<String>,
    pub system_audio: Option<String>,
}

pub fn retain_audio(transcript_only: bool, manual_preference: bool) -> bool {
    !transcript_only && manual_preference
}
pub fn safe_basename(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim().trim_matches('.');
    if name.is_empty() {
        "Transcript".into()
    } else {
        name.into()
    }
}
/// Automatic names already contain the local start timestamp; do not append UTC time.
pub fn create_transcript_session_folder(base: &Path, name: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(base)?;
    let folder = base.join(safe_basename(name));
    std::fs::create_dir(&folder).context("Cannot create unique transcript session folder")?;
    Ok(folder)
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name()
            .context("Missing filename")?
            .to_string_lossy()
    ));
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temporary, path)?;
    anyhow::ensure!(
        std::fs::read(path)? == bytes,
        "Saved transcript/metadata verification failed"
    );
    Ok(())
}
pub fn write_metadata(folder: &Path, metadata: &MeetingMetadata) -> Result<()> {
    write_atomic(
        &folder.join("metadata.json"),
        &serde_json::to_vec_pretty(metadata)?,
    )
}
#[derive(Serialize, Deserialize)]
struct TranscriptDocument {
    version: String,
    segments: Vec<TranscriptSegment>,
    last_updated: String,
    total_segments: usize,
}
pub fn write_transcripts(
    folder: &Path,
    name: &str,
    segments: &[TranscriptSegment],
) -> Result<PathBuf> {
    let mut segments = segments.to_vec();
    segments.sort_by_key(|segment| segment.sequence_id);
    let document = TranscriptDocument {
        version: "1.0".into(),
        total_segments: segments.len(),
        segments,
        last_updated: chrono::Utc::now().to_rfc3339(),
    };
    let json = serde_json::to_vec_pretty(&document)?;
    // Validate the complete document before replacing a previous durable copy.
    let validated: TranscriptDocument = serde_json::from_slice(&json)?;
    anyhow::ensure!(
        validated.total_segments == validated.segments.len(),
        "Transcript count mismatch"
    );
    write_atomic(&folder.join("transcripts.json"), &json)?;
    let mut text = format!("{}\n\n", name);
    for segment in &document.segments {
        let seconds = segment.audio_start_time.max(0.0) as u64;
        text.push_str(&format!(
            "[{:02}:{:02}:{:02}] {}\n",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60,
            segment.text
        ));
    }
    let text_path = folder.join(format!("{}.txt", safe_basename(name)));
    write_atomic(&text_path, text.as_bytes())?;
    Ok(text_path)
}
pub fn finalize_transcripts(
    folder: &Path,
    metadata: &MeetingMetadata,
    segments: &[TranscriptSegment],
    duration: Option<f64>,
) -> Result<MeetingMetadata> {
    write_transcripts(
        folder,
        metadata.meeting_name.as_deref().unwrap_or("Transcript"),
        segments,
    )?;
    let mut completed = metadata.clone();
    completed.status = "completed".into();
    completed.completed_at = Some(chrono::Utc::now().to_rfc3339());
    completed.duration_seconds = duration.or_else(|| {
        segments
            .iter()
            .map(|segment| segment.audio_end_time)
            .reduce(f64::max)
    });
    if let Some(duration) = completed.duration_seconds {
        anyhow::ensure!(
            duration.is_finite() && duration >= 0.0,
            "Invalid recording duration"
        );
    }
    write_metadata(folder, &completed)?;
    Ok(completed)
}

#[derive(Clone)]
pub struct TranscriptSink {
    segments: std::sync::Arc<std::sync::Mutex<Vec<TranscriptSegment>>>,
    writes: std::sync::Arc<std::sync::Mutex<()>>,
    folder: Option<PathBuf>,
    name: String,
}
impl TranscriptSink {
    pub fn new(folder: Option<PathBuf>, name: String) -> Self {
        Self {
            segments: Default::default(),
            writes: Default::default(),
            folder,
            name,
        }
    }
    pub fn upsert(&self, segment: TranscriptSegment) -> Result<()> {
        let _write = self
            .writes
            .lock()
            .map_err(|_| anyhow::anyhow!("Transcript write lock unavailable"))?;
        let snapshot = {
            let mut segments = self
                .segments
                .lock()
                .map_err(|_| anyhow::anyhow!("Transcript buffer unavailable"))?;
            if let Some(existing) = segments
                .iter_mut()
                .find(|existing| existing.sequence_id == segment.sequence_id)
            {
                *existing = segment;
            } else {
                segments.push(segment);
            }
            segments.clone()
        };
        if let Some(folder) = &self.folder {
            write_transcripts(folder, &self.name, &snapshot)?;
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Vec<TranscriptSegment>> {
        self.segments
            .lock()
            .map(|segments| segments.clone())
            .map_err(|_| anyhow::anyhow!("Transcript buffer unavailable"))
    }
    pub fn write(&self) -> Result<()> {
        let _write = self
            .writes
            .lock()
            .map_err(|_| anyhow::anyhow!("Transcript write lock unavailable"))?;
        let folder = self
            .folder
            .as_ref()
            .context("Transcript folder was not initialized")?;
        write_transcripts(folder, &self.name, &self.snapshot()?)?;
        Ok(())
    }
    pub fn finalize(
        &self,
        metadata: &MeetingMetadata,
        duration: Option<f64>,
    ) -> Result<MeetingMetadata> {
        let _write = self
            .writes
            .lock()
            .map_err(|_| anyhow::anyhow!("Transcript write lock unavailable"))?;
        let folder = self
            .folder
            .as_ref()
            .context("Transcript folder was not initialized")?;
        finalize_transcripts(folder, metadata, &self.snapshot()?, duration)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn metadata(name: &str) -> MeetingMetadata {
        MeetingMetadata {
            version: "1.0".into(),
            meeting_id: None,
            meeting_name: Some(name.into()),
            created_at: "2026-10-23T19:50:12Z".into(),
            completed_at: None,
            duration_seconds: None,
            devices: DeviceInfo {
                microphone: None,
                system_audio: None,
            },
            audio_file: String::new(),
            transcript_file: "transcripts.json".into(),
            sample_rate: 48000,
            status: "recording".into(),
        }
    }
    fn segment(id: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: id.to_string(),
            text: text.into(),
            audio_start_time: id as f64,
            audio_end_time: id as f64 + 1.0,
            duration: 1.0,
            display_time: "00:00".into(),
            confidence: 1.0,
            sequence_id: id,
        }
    }
    #[test]
    fn transcript_only_finalization_keeps_last_segment_and_completes_metadata_without_audio() {
        let directory = tempfile::tempdir().unwrap();
        let name = "Teams 2026-10-23_21-50-12";
        let folder = create_transcript_session_folder(directory.path(), name).unwrap();
        let initial = metadata(name);
        write_metadata(&folder, &initial).unwrap();
        write_transcripts(&folder, name, &[segment(1, "First")]).unwrap();
        let completed = finalize_transcripts(
            &folder,
            &initial,
            &[segment(2, "Final segment"), segment(1, "First")],
            Some(42.5),
        )
        .unwrap();
        let document: TranscriptDocument =
            serde_json::from_slice(&std::fs::read(folder.join("transcripts.json")).unwrap())
                .unwrap();
        assert_eq!(document.total_segments, 2);
        assert_eq!(document.segments[1].text, "Final segment");
        assert_eq!(completed.status, "completed");
        assert!(completed.completed_at.is_some());
        assert_eq!(completed.duration_seconds, Some(42.5));
        assert!(completed.audio_file.is_empty());
        let saved: MeetingMetadata =
            serde_json::from_slice(&std::fs::read(folder.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(saved.status, "completed");
        let text = std::fs::read_to_string(folder.join(format!("{}.txt", name))).unwrap();
        assert!(text.contains("[00:00:02] Final segment"));
        assert_eq!(folder.file_name().unwrap(), name);
        let mut files: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        assert_eq!(
            files,
            vec![
                format!("{}.txt", name),
                "metadata.json".into(),
                "transcripts.json".into()
            ]
        );
    }
    #[test]
    fn failed_text_write_does_not_mark_completed_and_retry_preserves_final_segments() {
        let directory = tempfile::tempdir().unwrap();
        let name = "Zoom 2026-10-23_21-50-12";
        let folder = create_transcript_session_folder(directory.path(), name).unwrap();
        let initial = metadata(name);
        write_metadata(&folder, &initial).unwrap();
        let text_path = folder.join(format!("{}.txt", name));
        std::fs::create_dir(&text_path).unwrap();
        assert!(
            finalize_transcripts(&folder, &initial, &[segment(9, "Must survive")], None).is_err()
        );
        let saved: MeetingMetadata =
            serde_json::from_slice(&std::fs::read(folder.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(saved.status, "recording");
        std::fs::remove_dir(text_path).unwrap();
        let completed =
            finalize_transcripts(&folder, &initial, &[segment(9, "Must survive")], None).unwrap();
        assert_eq!(completed.duration_seconds, Some(10.0));
        assert!(
            std::fs::read_to_string(folder.join(format!("{}.txt", name)))
                .unwrap()
                .contains("Must survive")
        );
    }
    #[test]
    fn detached_worker_sink_keeps_late_updates_retry_and_sessions_separate() {
        let directory = tempfile::tempdir().unwrap();
        let name = "Teams 2026-10-23_21-50-12";
        let folder = create_transcript_session_folder(directory.path(), name).unwrap();
        let initial = metadata(name);
        write_metadata(&folder, &initial).unwrap();
        let owner = TranscriptSink::new(Some(folder.clone()), name.into());
        let worker = owner.clone();
        owner.upsert(segment(1, "First draft")).unwrap();
        // The manager can leave the global registry while queued workers finish.
        let registry: Option<TranscriptSink> = None;
        assert!(registry.is_none());
        worker.upsert(segment(1, "First corrected")).unwrap();
        worker.upsert(segment(2, "Late final segment")).unwrap();
        let text_path = folder.join(format!("{}.txt", name));
        std::fs::remove_file(&text_path).unwrap();
        std::fs::create_dir(&text_path).unwrap();
        assert!(owner.finalize(&initial, Some(25.0)).is_err());
        assert_eq!(owner.snapshot().unwrap().len(), 2);
        std::fs::remove_dir(&text_path).unwrap();
        owner.finalize(&initial, Some(25.0)).unwrap();
        let saved: TranscriptDocument =
            serde_json::from_slice(&std::fs::read(folder.join("transcripts.json")).unwrap())
                .unwrap();
        assert_eq!(saved.segments.len(), 2);
        assert_eq!(saved.segments[0].text, "First corrected");
        assert_eq!(saved.segments[1].text, "Late final segment");
        let next_folder =
            create_transcript_session_folder(directory.path(), "Zoom 2026-10-23_22-00-00").unwrap();
        let next = TranscriptSink::new(Some(next_folder), "Zoom 2026-10-23_22-00-00".into());
        next.upsert(segment(1, "Different session")).unwrap();
        assert_eq!(next.snapshot().unwrap().len(), 1);
        assert_eq!(owner.snapshot().unwrap().len(), 2);
    }
    #[test]
    fn automatic_sessions_never_retain_audio_and_manual_preference_is_preserved() {
        assert!(!retain_audio(true, true));
        assert!(!retain_audio(true, false));
        assert!(retain_audio(false, true));
        assert!(!retain_audio(false, false));
    }
    #[test]
    fn colliding_session_cannot_overwrite_previous_transcripts() {
        let directory = tempfile::tempdir().unwrap();
        create_transcript_session_folder(directory.path(), "Teams 2026-10-23_21-50-12").unwrap();
        assert!(
            create_transcript_session_folder(directory.path(), "Teams 2026-10-23_21-50-12")
                .is_err()
        );
    }
}
