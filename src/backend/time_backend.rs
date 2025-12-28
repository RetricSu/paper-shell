use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// Messages sent to the time tracking thread
pub enum TimeMessage {
    /// Update focus state: true for focused, false for not focused
    FocusUpdate(bool),
    /// Stop the time tracking thread
    Stop,
}

/// Time backend for tracking writing time when editor is focused
pub struct TimeBackend {
    /// Total writing time in milliseconds
    writing_time: Arc<AtomicU64>,
    /// Sender to communicate with the time tracking thread
    sender: Sender<TimeMessage>,
    /// Handle to the time tracking thread
    _thread_handle: thread::JoinHandle<()>,
}

impl TimeBackend {
    /// Create a new TimeBackend
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let writing_time = Arc::new(AtomicU64::new(0));

        let writing_time_clone = Arc::clone(&writing_time);
        let thread_handle = thread::spawn(move || {
            Self::time_tracking_loop(receiver, writing_time_clone);
        });

        Self {
            writing_time,
            sender,
            _thread_handle: thread_handle,
        }
    }

    /// Get the current writing time in seconds and reset the counter
    pub fn get_and_reset_writing_time(&self) -> u64 {
        let time_ms = self.writing_time.swap(0, Ordering::Relaxed);
        time_ms / 1000
    }

    /// Get the current writing time in seconds
    pub fn get_writing_time(&self) -> u64 {
        self.writing_time.load(Ordering::Relaxed) / 1000
    }

    /// Update the focus state
    pub fn update_focus(&self, focused: bool) {
        let _ = self.sender.send(TimeMessage::FocusUpdate(focused));
    }

    /// The main time tracking loop that runs in a separate thread
    fn time_tracking_loop(receiver: Receiver<TimeMessage>, writing_time: Arc<AtomicU64>) {
        let mut is_focused = false;
        let mut focus_start_time = Instant::now();

        loop {
            // Check for messages with a timeout
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(TimeMessage::FocusUpdate(focused)) => {
                    if focused && !is_focused {
                        // Just gained focus, start timing
                        focus_start_time = Instant::now();
                    } else if !focused && is_focused {
                        // Just lost focus, add accumulated time
                        let elapsed_ms = focus_start_time.elapsed().as_millis() as u64;
                        writing_time.fetch_add(elapsed_ms, Ordering::Relaxed);
                    }
                    is_focused = focused;
                }
                Ok(TimeMessage::Stop) => {
                    // Add any remaining time before stopping
                    if is_focused {
                        let elapsed_ms = focus_start_time.elapsed().as_millis() as u64;
                        writing_time.fetch_add(elapsed_ms, Ordering::Relaxed);
                    }
                    break;
                }
                Err(_) => {
                    // Timeout, no action needed - timing is handled on focus changes
                }
            }
        }
    }
}

impl Default for TimeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TimeBackend {
    fn drop(&mut self) {
        let _ = self.sender.send(TimeMessage::Stop);
        // Note: We don't wait for the thread to join in drop to avoid blocking
        // The thread will be joined when the program exits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_time_accumulation() {
        let backend = TimeBackend::new();

        // Initially should be 0
        assert_eq!(backend.get_writing_time(), 0);

        // Simulate focus
        backend.update_focus(true);
        thread::sleep(Duration::from_millis(1100)); // Sleep for more than 1 second
        backend.update_focus(false);
        thread::sleep(Duration::from_millis(200)); // Allow thread to process

        // Should have accumulated at least 1 second
        let time = backend.get_writing_time();
        assert!(time >= 1, "Expected at least 1 second, got {}", time);

        // Simulate focus again
        backend.update_focus(true);
        thread::sleep(Duration::from_millis(1500));
        backend.update_focus(false);
        thread::sleep(Duration::from_millis(200)); // Allow thread to process

        // Should have more time now
        let new_time = backend.get_writing_time();
        assert!(
            new_time > time,
            "Expected more time after second focus period, was {} now {}",
            time,
            new_time
        );
    }

    #[test]
    fn test_format_writing_time() {
        // Test seconds and minutes
        assert_eq!(format_writing_time(0), "00:00");
        assert_eq!(format_writing_time(59), "00:59");
        assert_eq!(format_writing_time(60), "01:00");
        assert_eq!(format_writing_time(3599), "59:59");
        assert_eq!(format_writing_time(3600), "01:00:00");
        assert_eq!(format_writing_time(7265), "02:01:05");
    }

    fn format_writing_time(seconds: u64) -> String {
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        if hours > 0 {
            format!("{:02}:{:02}:{:02}", hours, minutes, secs)
        } else {
            format!("{:02}:{:02}", minutes, secs)
        }
    }
}
