use super::super::sanitize_terminal_text;

#[test]
fn sanitizer_escapes_terminal_and_direction_controls() {
    let value = "safe\n\u{1b}[31mred\u{202e}text\u{200b}\u{061c}";
    let sanitized = sanitize_terminal_text(value);
    assert_eq!(
        sanitized,
        "safe\\n\\u{001B}[31mred\\u{202E}text\\u{200B}\\u{061C}"
    );
    assert!(!sanitized.contains('\u{1b}'));
    assert!(!sanitized.contains('\u{202e}'));
    assert!(!sanitized.contains('\u{061c}'));
}
