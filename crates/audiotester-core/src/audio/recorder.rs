//! Sample recording for independent loss verification
//!
//! Records all sent and received counter samples to binary files on disk,
//! enabling post-hoc verification that the loss detection algorithm is correct.
//!
//! ## File Format
//!
//! Each record is 10 bytes: `[u16_le counter][u64_le frame_index]`
//!
//! Files are named `sent_YYYYMMDD_HHMMSS.bin` and `recv_YYYYMMDD_HHMMSS.bin`,
//! rotated every 10 minutes with 1-hour retention.

use super::engine::RecordEntry;
use ringbuf::traits::{Consumer, Observer};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Handle returned by [`SampleRecorder::start`] to stop recording
pub struct RecorderHandle {
    stop_flag: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RecorderHandle {
    /// Stop the recording thread and wait for it to finish
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

impl Drop for RecorderHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Always-on sample recorder for loss verification
pub struct SampleRecorder {
    dir: PathBuf,
    retention: Duration,
    file_duration: Duration,
}

impl SampleRecorder {
    /// Create a new recorder writing to the given directory
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            retention: Duration::from_secs(3600),    // 1 hour
            file_duration: Duration::from_secs(600), // 10 minutes
        }
    }

    /// Spawn the recording thread. Returns a handle to stop it.
    pub fn start(
        self,
        sent_consumer: ringbuf::HeapCons<RecordEntry>,
        recv_consumer: ringbuf::HeapCons<RecordEntry>,
    ) -> RecorderHandle {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&stop_flag);

        let thread = std::thread::Builder::new()
            .name("sample-recorder".into())
            .spawn(move || {
                self.recording_loop(sent_consumer, recv_consumer, flag_clone);
            })
            .expect("Failed to spawn sample recorder thread");

        RecorderHandle {
            stop_flag,
            thread: Some(thread),
        }
    }

    fn recording_loop(
        &self,
        mut sent_consumer: ringbuf::HeapCons<RecordEntry>,
        mut recv_consumer: ringbuf::HeapCons<RecordEntry>,
        stop_flag: Arc<AtomicBool>,
    ) {
        // Ensure recording directory exists
        if let Err(e) = fs::create_dir_all(&self.dir) {
            tracing::error!(error = %e, "Failed to create recordings directory");
            return;
        }

        tracing::info!(dir = %self.dir.display(), "Sample recorder started");

        let mut sent_buf = vec![
            RecordEntry {
                counter: 0,
                frame_index: 0
            };
            4096
        ];
        let mut recv_buf = vec![
            RecordEntry {
                counter: 0,
                frame_index: 0
            };
            4096
        ];

        let mut sent_writer: Option<BufWriter<File>> = None;
        let mut recv_writer: Option<BufWriter<File>> = None;
        let mut file_opened_at = std::time::Instant::now();

        // Open initial files
        if let Some((sw, rw)) = self.open_file_pair() {
            sent_writer = Some(sw);
            recv_writer = Some(rw);
            file_opened_at = std::time::Instant::now();
        }

        loop {
            if stop_flag.load(Ordering::Acquire) {
                break;
            }

            // Drain sent ring buffer
            let sent_available = sent_consumer.occupied_len();
            if sent_available > 0 {
                let to_read = sent_available.min(sent_buf.len());
                let read = sent_consumer.pop_slice(&mut sent_buf[..to_read]);
                if let Some(ref mut w) = sent_writer {
                    for entry in &sent_buf[..read] {
                        let _ = w.write_all(&entry.counter.to_le_bytes());
                        let _ = w.write_all(&entry.frame_index.to_le_bytes());
                    }
                }
            }

            // Drain recv ring buffer
            let recv_available = recv_consumer.occupied_len();
            if recv_available > 0 {
                let to_read = recv_available.min(recv_buf.len());
                let read = recv_consumer.pop_slice(&mut recv_buf[..to_read]);
                if let Some(ref mut w) = recv_writer {
                    for entry in &recv_buf[..read] {
                        let _ = w.write_all(&entry.counter.to_le_bytes());
                        let _ = w.write_all(&entry.frame_index.to_le_bytes());
                    }
                }
            }

            // Rotate files if needed
            if file_opened_at.elapsed() >= self.file_duration {
                // Flush and close current files
                if let Some(ref mut w) = sent_writer {
                    let _ = w.flush();
                }
                if let Some(ref mut w) = recv_writer {
                    let _ = w.flush();
                }

                // Open new files
                if let Some((sw, rw)) = self.open_file_pair() {
                    sent_writer = Some(sw);
                    recv_writer = Some(rw);
                } else {
                    sent_writer = None;
                    recv_writer = None;
                }
                file_opened_at = std::time::Instant::now();

                // Clean up old files
                self.cleanup_old_files();
            }

            // Sleep 10ms between drains
            std::thread::sleep(Duration::from_millis(10));
        }

        // Flush on exit
        if let Some(ref mut w) = sent_writer {
            let _ = w.flush();
        }
        if let Some(ref mut w) = recv_writer {
            let _ = w.flush();
        }

        tracing::info!("Sample recorder stopped");
    }

    fn open_file_pair(&self) -> Option<(BufWriter<File>, BufWriter<File>)> {
        let now = chrono::Local::now();
        let timestamp = now.format("%Y%m%d_%H%M%S");

        let sent_path = self.dir.join(format!("sent_{}.bin", timestamp));
        let recv_path = self.dir.join(format!("recv_{}.bin", timestamp));

        let sent_file = match File::create(&sent_path) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(path = %sent_path.display(), error = %e, "Failed to create sent recording file");
                return None;
            }
        };

        let recv_file = match File::create(&recv_path) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(path = %recv_path.display(), error = %e, "Failed to create recv recording file");
                return None;
            }
        };

        tracing::debug!(
            sent = %sent_path.display(),
            recv = %recv_path.display(),
            "Opened new recording files"
        );

        Some((
            BufWriter::with_capacity(8192, sent_file),
            BufWriter::with_capacity(8192, recv_file),
        ))
    }

    fn cleanup_old_files(&self) {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let now = SystemTime::now();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().map(|e| e == "bin").unwrap_or(false) {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > self.retention {
                            if let Err(e) = fs::remove_file(&path) {
                                tracing::warn!(
                                    path = %path.display(),
                                    error = %e,
                                    "Failed to remove old recording file"
                                );
                            } else {
                                tracing::debug!(
                                    path = %path.display(),
                                    age_secs = age.as_secs(),
                                    "Removed old recording file"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
