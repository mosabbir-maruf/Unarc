//! Production-grade interactive password prompt for Unarc.
//!
//! Provides dynamic character count, masked/unmasked toggling, keyboard accessibility,
//! Unicode cursor editing, narrow terminal support, and strict security compliance.

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{IsTerminal, Result as IoResult, Write, stdout};

/// Eye-style toggle glyph (Unicode Fisheye).
pub const EYE_TOGGLE_GLYPH: &str = "◉";

/// Bullet glyph for masked password characters.
pub const MASK_BULLET_GLYPH: &str = "•";

/// Encapsulates the state and editing operations of the password prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordPromptState {
    password: String,
    cursor: usize,
    visible: bool,
    left_margin: usize,
}

impl PasswordPromptState {
    /// Creates a new password prompt state, masked by default.
    #[must_use]
    pub fn new(left_margin: usize) -> Self {
        Self {
            password: String::new(),
            cursor: 0,
            visible: false,
            left_margin,
        }
    }

    /// Resets the prompt state, clearing the password and resetting visibility to masked.
    pub fn reset(&mut self) {
        self.password.clear();
        self.cursor = 0;
        self.visible = false;
    }

    /// Returns the current password string.
    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Returns whether the password is currently visible (unmasked).
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Sets visibility explicitly.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Toggles visibility between masked and unmasked without modifying the password value.
    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    /// Returns the current character count of the password.
    #[must_use]
    pub fn char_count(&self) -> usize {
        self.password.chars().count()
    }

    /// Returns the current character cursor position (0..=char_count).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Inserts a character at the current cursor position and advances the cursor.
    pub fn insert_char(&mut self, c: char) {
        let byte_idx = char_to_byte_index(&self.password, self.cursor);
        self.password.insert(byte_idx, c);
        self.cursor += 1;
    }

    /// Inserts a string (e.g. from bracketed paste) at the current cursor position.
    /// Strips trailing carriage returns and newlines to avoid unintended premature submission.
    pub fn insert_str(&mut self, text: &str) {
        let first_line = text.lines().next().unwrap_or("");
        let clean = first_line.trim_matches('\r');
        if clean.is_empty() {
            return;
        }
        let byte_idx = char_to_byte_index(&self.password, self.cursor);
        self.password.insert_str(byte_idx, clean);
        self.cursor += clean.chars().count();
    }

    /// Deletes the character before the cursor (backspace).
    /// Returns true if a character was removed.
    pub fn delete_backward(&mut self) -> bool {
        if self.cursor > 0 {
            let start_byte = char_to_byte_index(&self.password, self.cursor - 1);
            let end_byte = char_to_byte_index(&self.password, self.cursor);
            self.password.drain(start_byte..end_byte);
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    /// Deletes the character at the cursor (delete).
    /// Returns true if a character was removed.
    pub fn delete_forward(&mut self) -> bool {
        let count = self.char_count();
        if self.cursor < count {
            let start_byte = char_to_byte_index(&self.password, self.cursor);
            let end_byte = char_to_byte_index(&self.password, self.cursor + 1);
            self.password.drain(start_byte..end_byte);
            true
        } else {
            false
        }
    }

    /// Moves the cursor left by one character.
    pub fn move_cursor_left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    /// Moves the cursor right by one character.
    pub fn move_cursor_right(&mut self) -> bool {
        let count = self.char_count();
        if self.cursor < count {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Moves the cursor to the beginning of the input.
    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the cursor to the end of the input.
    pub fn move_cursor_end(&mut self) {
        self.cursor = self.char_count();
    }

    /// Renders the exact plain visual pattern without ANSI escape sequences:
    /// `Password  ••••••••••  (10)  ◉`
    #[must_use]
    pub fn render_plain(&self) -> String {
        let count = self.char_count();
        let val: String = if self.visible {
            self.password.clone()
        } else {
            MASK_BULLET_GLYPH.repeat(count)
        };
        let prefix = if self.left_margin > 0 {
            format!("{:width$}", "", width = self.left_margin)
        } else {
            String::new()
        };
        format!("{prefix}Password  {val}  ({count})  {EYE_TOGGLE_GLYPH}")
    }

    /// Renders the styled line and computes the cursor column for the terminal cursor.
    ///
    /// Respects `term_width` to prevent multi-line wrapping in narrow terminals,
    /// and respects `color_enabled` (NO_COLOR compliance).
    #[must_use]
    pub fn render_styled(&self, term_width: usize, color_enabled: bool) -> (String, usize) {
        let count = self.char_count();
        let count_str = format!("({count})");
        let fixed_right_len = 2 + count_str.len() + 2 + EYE_TOGGLE_GLYPH.chars().count(); // "  ({count})  ◉"
        let label_len = 8 + 2; // "Password  "
        let min_fixed = label_len + fixed_right_len;

        let effective_margin = if term_width > 0 {
            let space_left = term_width.saturating_sub(min_fixed);
            self.left_margin.min(space_left)
        } else {
            self.left_margin
        };

        let prefix = if effective_margin > 0 {
            format!("{:width$}", "", width = effective_margin)
        } else {
            String::new()
        };

        // Compute available space for the password characters
        let avail_pwd = if term_width > 0 {
            term_width.saturating_sub(effective_margin + min_fixed)
        } else {
            usize::MAX
        };

        let (win_start, win_len) = compute_visible_window(count, self.cursor, avail_pwd);

        let disp_chars: String = if self.visible {
            self.password
                .chars()
                .skip(win_start)
                .take(win_len)
                .collect()
        } else {
            MASK_BULLET_GLYPH.repeat(win_len)
        };

        let cursor_offset = self.cursor.saturating_sub(win_start);
        let cursor_col = effective_margin + label_len + cursor_offset;

        let line = if color_enabled {
            let label_styled = "\x1b[1;37mPassword\x1b[0m";
            let val_styled = if self.visible {
                format!("\x1b[1;37m{disp_chars}\x1b[0m")
            } else {
                format!("\x1b[1;36m{disp_chars}\x1b[0m")
            };
            let count_styled = format!("\x1b[90m{count_str}\x1b[0m");
            let icon_styled = if self.visible {
                format!("\x1b[1;36m{EYE_TOGGLE_GLYPH}\x1b[0m")
            } else {
                format!("\x1b[90m{EYE_TOGGLE_GLYPH}\x1b[0m")
            };
            format!("{prefix}{label_styled}  {val_styled}  {count_styled}  {icon_styled}")
        } else {
            format!("{prefix}Password  {disp_chars}  {count_str}  {EYE_TOGGLE_GLYPH}")
        };

        (line, cursor_col)
    }
}

/// Helper function to convert a character index to a byte index in a UTF-8 string.
fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

/// Computes the visible character window `(start, len)` to ensure the cursor remains visible
/// without exceeding `max_display_len`.
fn compute_visible_window(
    char_count: usize,
    cursor: usize,
    max_display_len: usize,
) -> (usize, usize) {
    if char_count <= max_display_len || max_display_len == 0 {
        (0, char_count)
    } else {
        let win_len = max_display_len;
        if cursor < win_len {
            (0, win_len)
        } else {
            let start = cursor.saturating_sub(win_len - 1);
            let bounded_start = start.min(char_count.saturating_sub(win_len));
            (bounded_start, win_len)
        }
    }
}

/// Formats a password prompt string using the exact pattern.
#[must_use]
pub fn render_password_prompt(left_margin: usize, password: &str, visible: bool) -> String {
    let mut state = PasswordPromptState::new(left_margin);
    state.insert_str(password);
    state.set_visible(visible);
    state.render_plain()
}

/// RAII guard ensuring terminal raw mode is cleanly restored on exit or panic.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Interactively prompts the user for an archive password in the terminal with full UX enhancements:
/// dynamic masked input, character count, toggleable visibility, Unicode navigation, and paste support.
pub fn prompt_password_terminal(left_margin: usize, color_enabled: bool) -> IoResult<String> {
    if !std::io::stdin().is_terminal() {
        return rpassword::read_password();
    }

    enable_raw_mode()?;
    let _guard = RawModeGuard;

    let mut state = PasswordPromptState::new(left_margin);
    let mut out = stdout();

    let redraw = |out: &mut std::io::Stdout, state: &PasswordPromptState| -> IoResult<()> {
        let (term_w, _) = crossterm::terminal::size().unwrap_or((80, 24));
        let (line, cursor_col) = state.render_styled(term_w as usize, color_enabled);
        write!(out, "\r\x1b[2K{line}")?;
        crossterm::queue!(
            out,
            crossterm::cursor::MoveToColumn(cursor_col as u16),
            crossterm::cursor::Show
        )?;
        out.flush()?;
        Ok(())
    };

    redraw(&mut out, &state)?;

    loop {
        match event::read()? {
            Event::Key(key) => {
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }

                // Global interrupt / abort (Ctrl+C / Ctrl+D)
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
                {
                    crate::platform::signals::set_interrupted(true);
                    drop(_guard);
                    writeln!(out, "\r")?;
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Password prompt interrupted",
                    ));
                }

                match key.code {
                    KeyCode::Enter => {
                        drop(_guard);
                        writeln!(out, "\r")?;
                        return Ok(state.password);
                    }
                    KeyCode::Esc => {
                        crate::platform::signals::set_interrupted(true);
                        drop(_guard);
                        writeln!(out, "\r")?;
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "Password prompt cancelled",
                        ));
                    }
                    // Show/hide toggle triggers: Tab, F2, Ctrl+T, Ctrl+E
                    KeyCode::Tab | KeyCode::F(2) => {
                        state.toggle_visibility();
                        redraw(&mut out, &state)?;
                    }
                    KeyCode::Char('t') | KeyCode::Char('e')
                        if key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        state.toggle_visibility();
                        redraw(&mut out, &state)?;
                    }
                    KeyCode::Char(c)
                        if !key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        state.insert_char(c);
                        redraw(&mut out, &state)?;
                    }
                    KeyCode::Backspace => {
                        if state.delete_backward() {
                            redraw(&mut out, &state)?;
                        }
                    }
                    KeyCode::Delete => {
                        if state.delete_forward() {
                            redraw(&mut out, &state)?;
                        }
                    }
                    KeyCode::Left => {
                        if state.move_cursor_left() {
                            redraw(&mut out, &state)?;
                        }
                    }
                    KeyCode::Right => {
                        if state.move_cursor_right() {
                            redraw(&mut out, &state)?;
                        }
                    }
                    KeyCode::Home => {
                        state.move_cursor_home();
                        redraw(&mut out, &state)?;
                    }
                    KeyCode::End => {
                        state.move_cursor_end();
                        redraw(&mut out, &state)?;
                    }
                    _ => {}
                }
            }
            Event::Paste(text) => {
                state.insert_str(&text);
                redraw(&mut out, &state)?;
            }
            Event::Resize(_, _) => {
                redraw(&mut out, &state)?;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regression_masked_rendering() {
        let mut state = PasswordPromptState::new(0);
        state.insert_str("secret1234");
        assert!(!state.is_visible());
        assert_eq!(state.render_plain(), "Password  ••••••••••  (10)  ◉");
        // Verify no cleartext characters exist in masked output
        assert!(!state.render_plain().contains("secret"));
    }

    #[test]
    fn test_regression_dynamic_character_count() {
        let mut state = PasswordPromptState::new(0);
        assert_eq!(state.char_count(), 0);
        assert_eq!(state.render_plain(), "Password    (0)  ◉");

        state.insert_char('a');
        assert_eq!(state.char_count(), 1);
        assert_eq!(state.render_plain(), "Password  •  (1)  ◉");

        state.insert_str("bcdef");
        assert_eq!(state.char_count(), 6);
        assert_eq!(state.render_plain(), "Password  ••••••  (6)  ◉");

        state.delete_backward();
        assert_eq!(state.char_count(), 5);
        assert_eq!(state.render_plain(), "Password  •••••  (5)  ◉");
    }

    #[test]
    fn test_regression_show_hide_toggle() {
        let mut state = PasswordPromptState::new(0);
        state.insert_str("secret1234");

        // Masked
        assert!(!state.is_visible());
        assert_eq!(state.render_plain(), "Password  ••••••••••  (10)  ◉");

        // Toggle to Visible
        state.toggle_visibility();
        assert!(state.is_visible());
        assert_eq!(state.render_plain(), "Password  secret1234  (10)  ◉");

        // Toggle back to Masked
        state.toggle_visibility();
        assert!(!state.is_visible());
        assert_eq!(state.render_plain(), "Password  ••••••••••  (10)  ◉");
    }

    #[test]
    fn test_regression_visibility_reset_on_new_prompt() {
        let mut state = PasswordPromptState::new(0);
        state.insert_str("my_secret");
        state.set_visible(true);
        assert!(state.is_visible());

        // Reset must restore masked state
        state.reset();
        assert!(!state.is_visible());
        assert_eq!(state.char_count(), 0);
        assert_eq!(state.password(), "");

        // Brand new state is always masked
        let fresh = PasswordPromptState::new(4);
        assert!(!fresh.is_visible());
        assert_eq!(fresh.char_count(), 0);
    }

    #[test]
    fn test_regression_password_value_unchanged_after_toggling() {
        let mut state = PasswordPromptState::new(0);
        let original_pwd = "P@$$w0rd!_123 🔑";
        state.insert_str(original_pwd);

        for _ in 0..10 {
            state.toggle_visibility();
            assert_eq!(state.password(), original_pwd);
        }
    }

    #[test]
    fn test_regression_cursor_navigation_and_editing() {
        let mut state = PasswordPromptState::new(0);
        state.insert_str("ac");
        assert_eq!(state.cursor(), 2);

        // Move left and insert 'b' between 'a' and 'c'
        state.move_cursor_left();
        assert_eq!(state.cursor(), 1);
        state.insert_char('b');
        assert_eq!(state.password(), "abc");
        assert_eq!(state.cursor(), 2);

        // Home and Delete 'a'
        state.move_cursor_home();
        assert_eq!(state.cursor(), 0);
        assert!(state.delete_forward());
        assert_eq!(state.password(), "bc");

        // End and Backspace
        state.move_cursor_end();
        assert_eq!(state.cursor(), 2);
        assert!(state.delete_backward());
        assert_eq!(state.password(), "b");
    }

    #[test]
    fn test_regression_bracketed_paste_handling() {
        let mut state = PasswordPromptState::new(0);
        // Multiline or carriage-return pasted input
        state.insert_str("PastedPass123\r\nextra line");
        assert_eq!(state.password(), "PastedPass123");
        assert_eq!(state.char_count(), 13);
    }

    #[test]
    fn test_regression_narrow_terminal_rendering() {
        let mut state = PasswordPromptState::new(4);
        state.insert_str("a_very_long_password_that_exceeds_narrow_width");

        // Narrow terminal width: 35 columns
        let (line_narrow, cursor_col) = state.render_styled(35, false);
        // Stripped width must be within 35 columns
        assert!(line_narrow.chars().count() <= 35);
        assert!(cursor_col <= 35);
        // Fixed tokens must be present
        assert!(line_narrow.contains("Password"));
        assert!(line_narrow.contains("◉"));
    }

    #[test]
    fn test_regression_no_color_behavior() {
        let mut state = PasswordPromptState::new(0);
        state.insert_str("plain_test");

        let (plain_line, _) = state.render_styled(80, false);
        assert!(!plain_line.contains("\x1b["));
        assert_eq!(plain_line, "Password  ••••••••••  (10)  ◉");

        state.toggle_visibility();
        let (plain_line_vis, _) = state.render_styled(80, false);
        assert!(!plain_line_vis.contains("\x1b["));
        assert_eq!(plain_line_vis, "Password  plain_test  (10)  ◉");
    }

    #[test]
    fn test_regression_unicode_password_handling() {
        let mut state = PasswordPromptState::new(0);
        state.insert_str("🦀🔑🛡️");
        // Multi-byte Unicode emojis
        assert_eq!(state.char_count(), 4);
        assert_eq!(state.render_plain(), "Password  ••••  (4)  ◉");

        state.toggle_visibility();
        assert_eq!(state.render_plain(), "Password  🦀🔑🛡️  (4)  ◉");
        assert_eq!(state.password(), "🦀🔑🛡️");
    }
}
