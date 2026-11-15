//! Embedded fonts used by the terminal.

use std::borrow::Cow;

/// Registers the embedded JetBrains Mono faces in the application text system.
pub fn load(cx: &mut gpui::App) -> Result<(), String> {
    cx.text_system()
        .add_fonts(vec![
            Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf").as_slice()),
            Cow::Borrowed(
                include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf").as_slice(),
            ),
        ])
        .map_err(|error| error.to_string())
}
