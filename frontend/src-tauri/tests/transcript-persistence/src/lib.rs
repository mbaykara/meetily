// Compile production persistence/saver modules without hardware capture or the model/Xcode stack.
#[path = "../../../src/audio/transcript_files.rs"]
pub mod transcript_files;
#[path = "../../../src/meeting_detection/session.rs"]
pub mod session;

#[path = "../../../src/audio/startup_cleanup.rs"]
pub mod startup_cleanup;
#[path = "../../../src/audio/recording_saver.rs"]
pub mod recording_saver;
#[path = "../../../src/audio/incremental_saver.rs"]
pub mod incremental_saver;
#[path = "../../../src/audio/encode.rs"]
pub mod encode;

// Hardware, folder selection and binary discovery are harness boundaries. The
// saver, incremental saver, codec invocation and disk persistence are production.
pub struct AudioDevice;
pub mod recording_state {
    #[derive(Clone)]
    pub enum DeviceType { Microphone, System }
    #[derive(Clone)]
    pub struct AudioChunk { pub data:Vec<f32>,pub sample_rate:u32,pub timestamp:f64,pub chunk_id:u64,pub device_type:DeviceType }
}
pub mod recording_preferences {
    pub fn get_default_recordings_folder() -> std::path::PathBuf { panic!("Folder selection is outside this harness") }
}
pub mod audio_processing {
    pub fn create_meeting_folder(_: &std::path::PathBuf,_: &str,_: bool) -> anyhow::Result<std::path::PathBuf> { panic!("Folder selection is outside this harness") }
}
pub mod ffmpeg {
    pub fn find_ffmpeg_path() -> Option<std::path::PathBuf> {
        std::env::var_os("MEETILY_TEST_FFMPEG").map(std::path::PathBuf::from)
            .or_else(|| { let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../binaries/ffmpeg-aarch64-apple-darwin"); path.is_file().then_some(path) })
    }
}
