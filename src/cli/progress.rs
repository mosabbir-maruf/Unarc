//! Terminal-native interactive progress indicator.

use std::io::Write;
use std::time::{Duration, Instant};

/// Terminal-native interactive progress indicator.
pub struct ProgressBar {
    operation: &'static str,
    start_time: Instant,
    last_render: Instant,
    last_pct: Option<u8>,
    color_enabled: bool,
    active: bool,
}

impl ProgressBar {
    /// Creates a new progress bar for the given operation label ("Extracting" or "Testing").
    #[must_use]
    pub fn new(operation: &'static str) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let now = Instant::now();
        let bar = Self {
            operation,
            start_time: now,
            last_render: now,
            last_pct: None,
            color_enabled: !no_color,
            active: true,
        };
        bar.render(0, None);
        bar
    }

    /// Updates the progress bar with a new percentage (0..=100) and optional current filename.
    pub fn update(&mut self, percentage: u8, current_file: Option<&str>) {
        if !self.active {
            return;
        }

        let now = Instant::now();
        let pct_changed = self.last_pct != Some(percentage);
        let time_elapsed = now.duration_since(self.last_render) >= Duration::from_millis(80);

        // Throttle updates: only re-render if percentage changed or >= 80ms elapsed
        if pct_changed || time_elapsed {
            self.render(percentage, current_file);
            self.last_pct = Some(percentage);
            self.last_render = now;
        }
    }

    /// Renders the progress line onto the terminal.
    fn render(&self, percentage: u8, current_file: Option<&str>) {
        let pct = percentage.min(100);
        let bar_width = 20usize;
        let filled = (pct as usize * bar_width) / 100;
        let unfilled = bar_width.saturating_sub(filled);

        let elapsed = self.start_time.elapsed().as_secs();
        let mins = elapsed / 60;
        let secs = elapsed % 60;
        let time_str = format!("[{mins:02}:{secs:02}]");

        // Query terminal width, defaulting to 80
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80)
            .max(40);

        let filled_bar: String = "█".repeat(filled);
        let unfilled_bar: String = "░".repeat(unfilled);

        // Calculate base text width: "Extracting: [████████████░░░░░░░░]  60% [00:15]"
        let base_len = self.operation.len() + 2 + 1 + bar_width + 1 + 6 + 1 + time_str.len();

        let file_part = if let Some(f) = current_file {
            let max_file_len = term_width.saturating_sub(base_len + 4);
            if max_file_len >= 10 {
                if f.len() > max_file_len {
                    let tail_len = max_file_len.saturating_sub(3);
                    format!(" (...{})", &f[f.len() - tail_len..])
                } else {
                    format!(" ({f})")
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if self.color_enabled {
            print!(
                "\r\x1b[2K\x1b[1;36m{}:\x1b[0m [\x1b[36m{}\x1b[90m{}\x1b[0m] \x1b[1m{:>3}%\x1b[0m \x1b[90m{}\x1b[0m\x1b[2m{}\x1b[0m",
                self.operation,
                filled_bar,
                unfilled_bar,
                pct,
                time_str,
                file_part
            );
        } else {
            print!(
                "\r\x1b[2K{}: [{}{}] {:>3}% {}{}",
                self.operation, filled_bar, unfilled_bar, pct, time_str, file_part
            );
        }
        let _ = std::io::stdout().flush();
    }

    /// Clears the progress line from the terminal.
    pub fn clear(&mut self) {
        if self.active {
            print!("\r\x1b[2K");
            let _ = std::io::stdout().flush();
            self.active = false;
        }
    }

    /// Completes the progress bar and clears the terminal line cleanly.
    pub fn finish(&mut self) {
        self.clear();
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        self.clear();
    }
}

impl crate::archive::ProgressListener for ProgressBar {
    fn on_progress(&mut self, percentage: u8, current_file: Option<&str>) {
        self.update(percentage, current_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_creation_and_update() {
        let mut bar = ProgressBar::new("Extracting");
        assert!(bar.active);
        bar.update(25, Some("file1.bin"));
        bar.update(50, Some("file2.bin"));
        bar.update(100, None);
        bar.finish();
        assert!(!bar.active);
    }
}
