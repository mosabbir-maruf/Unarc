//! Terminal-native interactive progress indicator.

use std::io::Write;
use std::time::{Duration, Instant};

const SPEED_WINDOW_CAPACITY: usize = 8;

#[derive(Debug, Clone, Copy)]
struct SpeedSample {
    time: Instant,
    bytes: u64,
}

#[derive(Debug)]
struct SpeedEstimator {
    samples: [Option<SpeedSample>; SPEED_WINDOW_CAPACITY],
    count: usize,
    head: usize,
}

impl SpeedEstimator {
    fn new() -> Self {
        Self {
            samples: [None; SPEED_WINDOW_CAPACITY],
            count: 0,
            head: 0,
        }
    }

    fn update_sample(&mut self, now: Instant, bytes: u64) {
        if self.count == 0 {
            self.samples[0] = Some(SpeedSample { time: now, bytes });
            self.count = 1;
            self.head = 0;
            return;
        }

        if let Some(last) = self.samples[self.head] {
            // Coalesce updates occurring within 100ms to avoid noisy zero-delta samples
            if now.duration_since(last.time) < Duration::from_millis(100) {
                if bytes >= last.bytes {
                    self.samples[self.head] = Some(SpeedSample { time: now, bytes });
                }
                return;
            }
        }

        self.head = (self.head + 1) % SPEED_WINDOW_CAPACITY;
        self.samples[self.head] = Some(SpeedSample { time: now, bytes });
        if self.count < SPEED_WINDOW_CAPACITY {
            self.count += 1;
        }
    }

    fn estimate_speed(&self, start_time: Instant, current_bytes: u64) -> Option<f64> {
        if self.count >= 2 {
            let oldest_idx =
                (self.head + SPEED_WINDOW_CAPACITY + 1 - self.count) % SPEED_WINDOW_CAPACITY;
            if let (Some(oldest), Some(newest)) =
                (self.samples[oldest_idx], self.samples[self.head])
            {
                let dt = newest.time.duration_since(oldest.time).as_secs_f64();
                let db = newest.bytes.saturating_sub(oldest.bytes);
                if dt >= 0.35 && db > 0 {
                    return Some(db as f64 / dt);
                }
            }
        }

        let total_dt = start_time.elapsed().as_secs_f64();
        if total_dt >= 0.5 && current_bytes > 0 {
            Some(current_bytes as f64 / total_dt)
        } else {
            None
        }
    }
}

/// Formats a processed/total byte pair into stable human-readable units (B, KB, MB, GB, TB).
/// The unit scales based on `total` to prevent line-width jitter as progress increments.
#[must_use]
pub fn format_progress_ratio(processed: u64, total: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const TB: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

    let tot = total as f64;
    let proc = (processed.min(total)) as f64;

    if tot < KB {
        format!("{processed} B / {total} B")
    } else if tot < MB {
        format!("{:.1} KB / {:.1} KB", proc / KB, tot / KB)
    } else if tot < GB {
        format!("{:.1} MB / {:.1} MB", proc / MB, tot / MB)
    } else if tot < TB {
        format!("{:.1} GB / {:.1} GB", proc / GB, tot / GB)
    } else {
        format!("{:.1} TB / {:.1} TB", proc / TB, tot / TB)
    }
}

/// Formats throughput in bytes per second into human-readable units (B/s, KB/s, MB/s, GB/s).
#[must_use]
pub fn format_throughput(bytes_per_sec: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    if bytes_per_sec < KB {
        format!("{:.0} B/s", bytes_per_sec.max(0.0))
    } else if bytes_per_sec < MB {
        let kb = bytes_per_sec / KB;
        if kb >= 100.0 {
            format!("{kb:.0} KB/s")
        } else {
            format!("{kb:.1} KB/s")
        }
    } else if bytes_per_sec < GB {
        let mb = bytes_per_sec / MB;
        if mb >= 100.0 {
            format!("{mb:.0} MB/s")
        } else {
            format!("{mb:.1} MB/s")
        }
    } else {
        let gb = bytes_per_sec / GB;
        if gb >= 100.0 {
            format!("{gb:.0} GB/s")
        } else {
            format!("{gb:.1} GB/s")
        }
    }
}

/// Formats remaining duration into human-readable ETA (e.g. "ETA 1s", "ETA 1m 24s", "ETA 2h 15m").
#[must_use]
pub fn format_eta(remaining_secs: u64) -> String {
    if remaining_secs < 60 {
        format!("ETA {remaining_secs}s")
    } else if remaining_secs < 3600 {
        let mins = remaining_secs / 60;
        let secs = remaining_secs % 60;
        if secs == 0 {
            format!("ETA {mins}m")
        } else {
            format!("ETA {mins}m {secs}s")
        }
    } else {
        let hours = remaining_secs / 3600;
        let mins = (remaining_secs % 3600) / 60;
        format!("ETA {hours}h {mins}m")
    }
}

const FULL_BAR: &str = "████████████████████";
const EMPTY_BAR: &str = "░░░░░░░░░░░░░░░░░░░░";

/// Terminal-native interactive progress indicator.
pub struct ProgressBar {
    operation: &'static str,
    start_time: Instant,
    last_render: Instant,
    last_pct: Option<u8>,
    current_file: Option<String>,
    total_bytes: Option<u64>,
    speed_estimator: SpeedEstimator,
    color_enabled: bool,
    active: bool,
    rendered_lines: usize,
    left_margin: usize,
}

impl ProgressBar {
    /// Creates a new progress bar for the given operation label with left margin indentation.
    #[must_use]
    pub fn new(operation: &'static str, left_margin: usize) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let now = Instant::now();
        let mut bar = Self {
            operation,
            start_time: now,
            last_render: now,
            last_pct: None,
            current_file: None,
            total_bytes: None,
            speed_estimator: SpeedEstimator::new(),
            color_enabled: !no_color,
            active: true,
            rendered_lines: 0,
            left_margin,
        };
        bar.render(0);
        bar
    }

    /// Sets or updates total bytes processed during the operation.
    pub fn set_total_bytes(&mut self, total_bytes: u64) {
        if total_bytes > 0 && self.total_bytes != Some(total_bytes) {
            self.total_bytes = Some(total_bytes);
            if self.active {
                self.render(self.last_pct.unwrap_or(0));
            }
        }
    }

    /// Builder pattern for setting total bytes.
    #[must_use]
    pub fn with_total_bytes(mut self, total_bytes: u64) -> Self {
        self.set_total_bytes(total_bytes);
        self
    }

    /// Updates the progress bar with a new percentage (0..=100) and optional current filename.
    pub fn update(&mut self, percentage: u8, current_file: Option<&str>) {
        if !self.active {
            return;
        }

        let now = Instant::now();
        self.last_pct = Some(percentage);
        if let Some(f) = current_file {
            // Avoid re-allocating String if filename has not changed
            if self.current_file.as_deref() != Some(f) {
                let sanitized = sanitize_terminal_text(f);
                if !sanitized.is_empty() {
                    self.current_file = Some(sanitized);
                }
            }
        }

        let bytes_processed = self
            .total_bytes
            .map(|tot| (percentage.min(100) as u64).saturating_mul(tot) / 100)
            .unwrap_or(0);

        self.speed_estimator.update_sample(now, bytes_processed);

        // Throttle rendered progress updates to ~250ms (or immediate on 100%)
        let elapsed_since_render = now.duration_since(self.last_render);
        if elapsed_since_render >= Duration::from_millis(250) || percentage == 100 {
            self.render(percentage);
            self.last_render = now;
        }
    }

    /// Renders the progress line(s) onto the terminal.
    fn render(&mut self, percentage: u8) {
        if !self.active {
            return;
        }

        let pct = percentage.min(100);

        // Query terminal width, defaulting to 80, min 30
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80)
            .max(30);

        let avail_width = term_width.saturating_sub(self.left_margin).max(20);

        let bar_width = if avail_width < 50 { 12 } else { 20 };
        let filled = (pct as usize * bar_width) / 100;
        let unfilled = bar_width.saturating_sub(filled);

        // Zero-allocation static slices for progress bar blocks (3 bytes per UTF-8 character)
        let filled_bar = &FULL_BAR[..filled * 3];
        let unfilled_bar = &EMPTY_BAR[..unfilled * 3];

        // Base prefix width: "Extracting: [███████████████████░]  97%"
        let prefix_plain_len = self.operation.len() + 2 + 1 + bar_width + 1 + 5;

        let file_part = if let Some(ref f) = self.current_file {
            let avail = avail_width.saturating_sub(prefix_plain_len + 4);
            if avail >= 8 {
                let truncated = truncate_filename(f, avail);
                format!(" ({truncated})")
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let line1_styled = if self.color_enabled {
            format!(
                "\x1b[1;36m{}:\x1b[0m [\x1b[36m{}\x1b[90m{}\x1b[0m] \x1b[1m{:>3}%\x1b[0m\x1b[2m{}\x1b[0m",
                self.operation, filled_bar, unfilled_bar, pct, file_part
            )
        } else {
            format!(
                "{}: [{}{}] {:>3}%{}",
                self.operation, filled_bar, unfilled_bar, pct, file_part
            )
        };

        let bytes_processed = self
            .total_bytes
            .map(|tot| (pct as u64).saturating_mul(tot) / 100)
            .unwrap_or(0);

        let speed = self
            .speed_estimator
            .estimate_speed(self.start_time, bytes_processed);

        let eta_secs = speed.and_then(|spd| {
            let total_elapsed = self.start_time.elapsed().as_secs_f64();
            if total_elapsed >= 1.0 && pct >= 2 && spd > 10.0 {
                self.total_bytes.and_then(|tot| {
                    if tot > bytes_processed {
                        let rem = tot - bytes_processed;
                        let s = (rem as f64 / spd).round() as u64;
                        if s < 86400 { Some(s) } else { None }
                    } else {
                        Some(0)
                    }
                })
            } else {
                None
            }
        });

        let line2_styled = self.render_stats_line(avail_width, bytes_processed, speed, eta_secs);

        let mut out = std::io::stdout();
        match self.rendered_lines {
            0 => {}
            1 => {
                let _ = write!(out, "\r");
            }
            lines => {
                let _ = write!(out, "\x1b[{}A\r", lines - 1);
            }
        }

        let _ = write!(
            out,
            "\x1b[2K{:width$}{line1_styled}",
            "",
            width = self.left_margin
        );

        if let Some(ref l2) = line2_styled {
            let _ = write!(
                out,
                "\n\x1b[2K{:width$}{l2}",
                "",
                width = self.left_margin
            );
            self.rendered_lines = 2;
        } else {
            self.rendered_lines = 1;
        }

        let _ = out.flush();
    }

    fn render_stats_line(
        &self,
        term_width: usize,
        bytes_processed: u64,
        speed: Option<f64>,
        eta_secs: Option<u64>,
    ) -> Option<String> {
        let total_bytes = self.total_bytes?;

        let ratio_str = format_progress_ratio(bytes_processed, total_bytes);
        let speed_str = speed.map(format_throughput);
        let eta_str = eta_secs.map(format_eta);

        let mut items = Vec::with_capacity(3);
        items.push(ratio_str);
        if let Some(s) = speed_str {
            items.push(s);
        }
        if let Some(e) = eta_str {
            items.push(e);
        }

        let sep_plain = "  •  ";
        let sep_colored = if self.color_enabled {
            "\x1b[90m  •  \x1b[0m"
        } else {
            sep_plain
        };

        while items.len() > 1 {
            let total_len: usize =
                items.iter().map(|s| s.len()).sum::<usize>() + (items.len() - 1) * sep_plain.len();
            if total_len < term_width.saturating_sub(2) {
                break;
            }
            items.pop();
        }

        let line = items.join(sep_colored);
        Some(line)
    }

    /// Clears the progress line(s) from the terminal.
    pub fn clear(&mut self) {
        if self.active {
            let mut out = std::io::stdout();
            match self.rendered_lines {
                0 => {}
                1 => {
                    let _ = write!(out, "\r\x1b[2K");
                }
                lines => {
                    let _ = write!(out, "\r\x1b[2K\x1b[{}A\r\x1b[2K", lines - 1);
                }
            }
            let _ = out.flush();
            self.rendered_lines = 0;
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

    fn set_total_bytes(&mut self, total_bytes: u64) {
        self.set_total_bytes(total_bytes);
    }
}

/// Sanitizes archive-derived terminal text to prevent ANSI escape sequence injection
/// and control character interference, while keeping valid Unicode filenames fully readable.
#[must_use]
pub fn sanitize_terminal_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ANSI escape sequence handling: strip escape sequences safely
            match chars.peek() {
                Some(&'[') => {
                    // CSI sequence: ESC [ [params: 0x30-0x3F]* [intermediate: 0x20-0x2F]* [final: 0x40-0x7E]
                    chars.next(); // consume '['
                    // Consume parameter characters ('0'..='?')
                    while let Some(&p) = chars.peek() {
                        if ('0'..='?').contains(&p) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Consume intermediate characters (' '..='/')
                    while let Some(&i) = chars.peek() {
                        if (' '..='/').contains(&i) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Consume final character ('@'..='~')
                    if let Some(&f) = chars.peek() {
                        if ('@'..='~').contains(&f) {
                            chars.next();
                        }
                    }
                }
                Some(&']') => {
                    // OSC sequence: ESC ] ... (terminated by BEL '\x07' or ST ESC '\')
                    chars.next(); // consume ']'
                    while let Some(&o) = chars.peek() {
                        if o == '\x07' {
                            chars.next();
                            break;
                        } else if o == '\x1b' {
                            chars.next();
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        } else if o == '\n' || o == '\r' {
                            // Unterminated OSC safety stop on newline
                            break;
                        } else {
                            chars.next();
                        }
                    }
                }
                Some(&next_c) if (' '..='/').contains(&next_c) => {
                    // 2-byte escape sequence with intermediate byte (e.g. charset selection ESC ( B)
                    chars.next();
                    if let Some(&f) = chars.peek() {
                        if ('0'..='~').contains(&f) {
                            chars.next();
                        }
                    }
                }
                Some(&next_c) if ('@'..='_').contains(&next_c) => {
                    // 2-character Fe escape sequence: ESC followed by single char in 0x40..=0x5F
                    chars.next();
                }
                _ => {
                    // Stray or unrecognized escape: do not emit ESC
                }
            }
        } else if is_non_printable_or_control(c) {
            // Replace non-printable ASCII and Unicode control characters with '?'
            out.push('?');
        } else {
            // Printable ASCII and valid Unicode characters preserved as-is
            out.push(c);
        }
    }

    out
}

/// Returns true if a character is an ASCII control, Unicode control, or non-printable formatting code.
fn is_non_printable_or_control(c: char) -> bool {
    c.is_control()
        || ('\u{200B}'..='\u{200F}').contains(&c)
        || ('\u{202A}'..='\u{202E}').contains(&c)
        || ('\u{2066}'..='\u{2069}').contains(&c)
        || c == '\u{FEFF}'
}

/// Truncates a filename to at most `max_chars` characters using readable tail truncation (`...tail`).
/// This function is strictly Unicode-safe: it measures and slices by `char` boundaries,
/// ensuring it never slices a UTF-8 string at an arbitrary byte index.
#[must_use]
pub fn truncate_filename(filename: &str, max_chars: usize) -> String {
    let char_count = filename.chars().count();
    if char_count <= max_chars {
        return filename.to_string();
    }

    if max_chars <= 3 {
        return filename.chars().take(max_chars).collect();
    }

    let tail_chars = max_chars.saturating_sub(3);
    let skip_chars = char_count.saturating_sub(tail_chars);
    let byte_offset = filename
        .char_indices()
        .nth(skip_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(0);

    format!("...{}", &filename[byte_offset..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_creation_and_update() {
        let mut bar = ProgressBar::new("Extracting", 0);
        assert!(bar.active);
        bar.update(25, Some("file1.bin"));
        bar.update(50, Some("file2.bin"));
        bar.update(100, None);
        bar.finish();
        assert!(!bar.active);
    }

    #[test]
    fn test_normal_filename_remaining_unchanged() {
        let input = "documents/reports/quarterly_audit_2026.pdf";
        assert_eq!(sanitize_terminal_text(input), input);

        let archive = "archives/backup-data-v1.0.tar.gz";
        assert_eq!(sanitize_terminal_text(archive), archive);

        // When within max_chars, truncation leaves filename unchanged
        assert_eq!(truncate_filename(input, 50), input);
    }

    #[test]
    fn test_control_and_ansi_escape_characters_sanitized() {
        // ANSI CSI color sequences stripped cleanly
        let colored = "\x1b[31;1mred_alert.txt\x1b[0m";
        assert_eq!(sanitize_terminal_text(colored), "red_alert.txt");

        // ANSI line-clearing / cursor movement stripped
        let cursor_moves = "malicious\x1b[2K\x1b[1Afile.zip";
        assert_eq!(sanitize_terminal_text(cursor_moves), "maliciousfile.zip");

        // ANSI OSC window title sequence stripped
        let osc_title = "payload_\x1b]0;PwnedTitle\x07_data.bin";
        assert_eq!(sanitize_terminal_text(osc_title), "payload__data.bin");

        // Control characters (CR, LF, TAB, NUL, BS, BEL) replaced with '?'
        let controls = "dir\r\nsub\tfile\0with\x08bell\x07.txt";
        assert_eq!(
            sanitize_terminal_text(controls),
            "dir??sub?file?with?bell?.txt"
        );

        // Unicode Right-to-Left Override (RLO) control replaced with '?'
        let rlo = "safe_invoice\u{202E}cod.exe";
        assert_eq!(sanitize_terminal_text(rlo), "safe_invoice?cod.exe");
    }

    #[test]
    fn test_long_ascii_filename_truncation() {
        let long_ascii = "very/deeply/nested/directory/structure/and/long_filename.bin";
        let truncated = truncate_filename(long_ascii, 25);
        assert_eq!(truncated.chars().count(), 25);
        assert!(truncated.starts_with("..."));
        assert!(truncated.ends_with("long_filename.bin"));

        // Boundary cases
        assert_eq!(truncate_filename("abcdefghij", 10), "abcdefghij");
        assert_eq!(truncate_filename("abcdefghij", 7), "...ghij");
    }

    #[test]
    fn test_long_unicode_filename_truncation() {
        // Multi-byte Unicode (Bengali, accented Latin, Japanese, emoji)
        let long_unicode = "ফোল্ডার/নথিপত্র/গবেষণা/বই_এবং_দলিলপত্র_২০২৬_খসড়া.pdf";
        let truncated = truncate_filename(long_unicode, 20);
        assert_eq!(truncated.chars().count(), 20);
        assert!(truncated.starts_with("..."));
        assert!(truncated.ends_with("২০২৬_খসড়া.pdf"));

        let multi_byte = "café_crème_über_größter_🦀_日本語_test_unicode_long_string.tar.gz";
        let trunc_mb = truncate_filename(multi_byte, 22);
        assert_eq!(trunc_mb.chars().count(), 22);
        assert!(trunc_mb.starts_with("..."));

        // Multi-byte string where naive byte slicing would slice inside code points
        let repeated_multibyte = "é".repeat(30);
        let trunc_accent = truncate_filename(&repeated_multibyte, 10);
        assert_eq!(trunc_accent.chars().count(), 10);
        assert_eq!(trunc_accent, format!("...{}", "é".repeat(7)));
    }

    #[test]
    fn test_unicode_filename_rendering_without_panic() {
        let mut bar = ProgressBar::new("Testing", 0);
        // Test diverse Unicode scripts, emoji, and control injections across multiple updates
        let test_cases = [
            "é",
            "éé",
            "éééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééééé",
            "🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀",
            "বাংলা_ফাইল_নাম_খুব_দীর্ঘ_একটি_পাথ_যা_টার্মিনাল_প্রস্থ_ছাড়িয়ে_যাবে_এবং_টেইল_ট্রাঙ্কেট_হবে.tar.gz",
            "日本語のファイル名がとても長くてターミナルの幅を超える場合のテストケース.tar.gz",
            "\x1b[31;1m\x1b[2K\r\n\t\0malicious_utf8_🦀_accent_é_test.zip\x1b[0m",
            "مرحبا_بالعالم_هذا_ملف_طويل_جدا_لاختبار_النظام.zip",
        ];

        for (i, case) in test_cases.iter().enumerate() {
            bar.update((i * 12).min(100) as u8, Some(case));
        }
        bar.finish();
    }

    #[test]
    fn test_format_progress_ratio() {
        assert_eq!(format_progress_ratio(0, 0), "0 B / 0 B");
        assert_eq!(format_progress_ratio(512, 1024), "0.5 KB / 1.0 KB");
        assert_eq!(
            format_progress_ratio(50 * 1024 * 1024, 100 * 1024 * 1024),
            "50.0 MB / 100.0 MB"
        );
        // Exact numbers from target design: 48.7 GB / 50.2 GB
        let proc = (48.7 * 1024.0 * 1024.0 * 1024.0) as u64;
        let tot = (50.2 * 1024.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(format_progress_ratio(proc, tot), "48.7 GB / 50.2 GB");

        // Terabytes
        let tb_proc = (1.5 * 1024.0 * 1024.0 * 1024.0 * 1024.0) as u64;
        let tb_tot = (2.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(format_progress_ratio(tb_proc, tb_tot), "1.5 TB / 2.0 TB");
    }

    #[test]
    fn test_format_throughput() {
        assert_eq!(format_throughput(450.0), "450 B/s");
        assert_eq!(format_throughput(45.5 * 1024.0), "45.5 KB/s");
        assert_eq!(format_throughput(120.0 * 1024.0), "120 KB/s");
        // Exact numbers from target design: 812 MB/s
        assert_eq!(format_throughput(812.0 * 1024.0 * 1024.0), "812 MB/s");
        assert_eq!(format_throughput(15.4 * 1024.0 * 1024.0), "15.4 MB/s");
        assert_eq!(
            format_throughput(1.2 * 1024.0 * 1024.0 * 1024.0),
            "1.2 GB/s"
        );
    }

    #[test]
    fn test_format_eta() {
        // Exact number from target design: ETA 1s
        assert_eq!(format_eta(1), "ETA 1s");
        assert_eq!(format_eta(45), "ETA 45s");
        assert_eq!(format_eta(60), "ETA 1m");
        assert_eq!(format_eta(90), "ETA 1m 30s");
        assert_eq!(format_eta(3600), "ETA 1h 0m");
        assert_eq!(format_eta(3665), "ETA 1h 1m");
    }

    #[test]
    fn test_speed_estimator_rolling_window() {
        let mut est = SpeedEstimator::new();
        let t0 = Instant::now();
        assert_eq!(est.estimate_speed(t0, 0), None);

        // Add initial sample
        est.update_sample(t0, 0);

        // Add sample after 500ms
        let t1 = t0 + Duration::from_millis(500);
        est.update_sample(t1, 10 * 1024 * 1024);

        let speed = est.estimate_speed(t0, 10 * 1024 * 1024);
        assert!(speed.is_some());
        let spd = speed.unwrap();
        // ~20 MB/s (10MB / 0.5s = 20MB/s)
        assert!((spd - (20.0 * 1024.0 * 1024.0)).abs() < 100_000.0);
    }

    #[test]
    fn test_progress_bar_with_total_bytes_and_stats() {
        let total = (50.2 * 1024.0 * 1024.0 * 1024.0) as u64;
        let mut bar = ProgressBar::new("Extracting", 0).with_total_bytes(total);
        assert_eq!(bar.total_bytes, Some(total));

        bar.update(25, Some("data1.bin"));
        bar.update(50, Some("data2.bin"));
        bar.update(97, Some("data3.bin"));
        bar.update(100, None);
        bar.finish();
        assert!(!bar.active);
    }

    #[test]
    fn test_render_stats_line_responsive_pruning() {
        let total = (50.2 * 1024.0 * 1024.0 * 1024.0) as u64;
        let processed = (48.7 * 1024.0 * 1024.0 * 1024.0) as u64;
        let bar = ProgressBar::new("Extracting", 0).with_total_bytes(total);

        // At 100 columns: all 3 items fit
        let line_wide =
            bar.render_stats_line(100, processed, Some(812.0 * 1024.0 * 1024.0), Some(1));
        assert!(line_wide.is_some());
        let text = line_wide.unwrap();
        assert!(text.contains("48.7 GB / 50.2 GB"));
        assert!(text.contains("812 MB/s"));
        assert!(text.contains("ETA 1s"));

        // At 30 columns: ETA and speed pruned to fit narrow terminal
        let line_narrow =
            bar.render_stats_line(30, processed, Some(812.0 * 1024.0 * 1024.0), Some(1));
        assert!(line_narrow.is_some());
        let text_narrow = line_narrow.unwrap();
        assert!(text_narrow.contains("48.7 GB / 50.2 GB"));
        assert!(!text_narrow.contains("ETA 1s"));
    }

    #[test]
    fn test_progress_bar_with_left_margin() {
        let mut bar = ProgressBar::new("Extracting", 12);
        assert_eq!(bar.left_margin, 12);
        let bar2 = ProgressBar::new("Testing", 8);
        assert_eq!(bar2.left_margin, 8);
        bar.finish();
    }
}
