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
                let sanitized = sanitize_terminal_text(f);
                if sanitized.is_empty() {
                    String::new()
                } else {
                    let truncated = truncate_filename(&sanitized, max_file_len);
                    format!(" ({truncated})")
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
                self.operation, filled_bar, unfilled_bar, pct, time_str, file_part
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
        let mut bar = ProgressBar::new("Extracting");
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
        let mut bar = ProgressBar::new("Testing");
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
}
