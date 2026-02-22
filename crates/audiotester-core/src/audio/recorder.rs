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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Handle returned by [`SampleRecorder::start`] to stop recording
pub struct RecorderHandle {
    stop_flag: Arc<AtomicBool>,
    /// Total records written (sent + recv), updated by recorder thread
    records_written: Arc<AtomicU64>,
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

    /// Check if the recorder thread is still alive
    pub fn is_alive(&self) -> bool {
        self.thread
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    /// Total records written so far (sent + recv)
    pub fn records_written(&self) -> u64 {
        self.records_written.load(Ordering::Relaxed)
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
        let records_written = Arc::new(AtomicU64::new(0));
        let records_clone = Arc::clone(&records_written);

        let thread = std::thread::Builder::new()
            .name("sample-recorder".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.recording_loop(sent_consumer, recv_consumer, flag_clone, records_clone);
                }));
                match result {
                    Ok(()) => tracing::info!("Sample recorder thread exited normally"),
                    Err(panic_info) => {
                        let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_info.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "unknown panic".to_string()
                        };
                        tracing::error!(panic = %msg, "Sample recorder thread PANICKED");
                    }
                }
            })
            .expect("Failed to spawn sample recorder thread");

        RecorderHandle {
            stop_flag,
            records_written,
            thread: Some(thread),
        }
    }

    fn recording_loop(
        &self,
        mut sent_consumer: ringbuf::HeapCons<RecordEntry>,
        mut recv_consumer: ringbuf::HeapCons<RecordEntry>,
        stop_flag: Arc<AtomicBool>,
        records_written: Arc<AtomicU64>,
    ) {
        // Ensure recording directory exists
        if let Err(e) = fs::create_dir_all(&self.dir) {
            tracing::error!(error = %e, "Failed to create recordings directory");
            return;
        }

        tracing::info!(dir = %self.dir.display(), "Sample recorder thread running");

        let mut sent_buf = vec![
            RecordEntry {
                counter: 0,
                frame_index: 0,
            };
            4096
        ];
        let mut recv_buf = vec![
            RecordEntry {
                counter: 0,
                frame_index: 0,
            };
            4096
        ];

        let mut sent_writer: Option<BufWriter<File>> = None;
        let mut recv_writer: Option<BufWriter<File>> = None;
        let mut file_opened_at = std::time::Instant::now();
        let mut total_sent: u64 = 0;
        let mut total_recv: u64 = 0;
        let mut stats_logged_at = std::time::Instant::now();

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
                        if let Err(e) = w.write_all(&entry.counter.to_le_bytes()) {
                            tracing::error!(error = %e, "Failed to write sent record");
                            break;
                        }
                        if let Err(e) = w.write_all(&entry.frame_index.to_le_bytes()) {
                            tracing::error!(error = %e, "Failed to write sent record frame_index");
                            break;
                        }
                    }
                }
                total_sent += read as u64;
            }

            // Drain recv ring buffer
            let recv_available = recv_consumer.occupied_len();
            if recv_available > 0 {
                let to_read = recv_available.min(recv_buf.len());
                let read = recv_consumer.pop_slice(&mut recv_buf[..to_read]);
                if let Some(ref mut w) = recv_writer {
                    for entry in &recv_buf[..read] {
                        if let Err(e) = w.write_all(&entry.counter.to_le_bytes()) {
                            tracing::error!(error = %e, "Failed to write recv record");
                            break;
                        }
                        if let Err(e) = w.write_all(&entry.frame_index.to_le_bytes()) {
                            tracing::error!(error = %e, "Failed to write recv record frame_index");
                            break;
                        }
                    }
                }
                total_recv += read as u64;
            }

            // Update shared counter
            records_written.store(total_sent + total_recv, Ordering::Relaxed);

            // Log stats every 60 seconds
            if stats_logged_at.elapsed() >= Duration::from_secs(60) {
                tracing::info!(
                    sent = total_sent,
                    recv = total_recv,
                    has_sent_writer = sent_writer.is_some(),
                    has_recv_writer = recv_writer.is_some(),
                    file_age_secs = file_opened_at.elapsed().as_secs(),
                    "Recorder stats"
                );
                stats_logged_at = std::time::Instant::now();
            }

            // Rotate files if needed
            if file_opened_at.elapsed() >= self.file_duration {
                tracing::info!(
                    sent = total_sent,
                    recv = total_recv,
                    "Rotating recording files"
                );

                // Flush current files explicitly before dropping
                if let Some(mut w) = sent_writer.take() {
                    if let Err(e) = w.flush() {
                        tracing::error!(error = %e, "Failed to flush sent file on rotation");
                    }
                }
                if let Some(mut w) = recv_writer.take() {
                    if let Err(e) = w.flush() {
                        tracing::error!(error = %e, "Failed to flush recv file on rotation");
                    }
                }

                // Open new files
                match self.open_file_pair() {
                    Some((sw, rw)) => {
                        sent_writer = Some(sw);
                        recv_writer = Some(rw);
                        tracing::info!("New recording files opened after rotation");
                    }
                    None => {
                        tracing::error!("Failed to open new recording files after rotation");
                    }
                }
                file_opened_at = std::time::Instant::now();

                // Clean up old files
                self.cleanup_old_files();
            }

            // Sleep 10ms between drains
            std::thread::sleep(Duration::from_millis(10));
        }

        // Flush on exit
        if let Some(mut w) = sent_writer.take() {
            let _ = w.flush();
        }
        if let Some(mut w) = recv_writer.take() {
            let _ = w.flush();
        }

        tracing::info!(
            sent = total_sent,
            recv = total_recv,
            "Sample recorder stopped"
        );
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
