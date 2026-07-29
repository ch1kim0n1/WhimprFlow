//! Optional dictation sound cues (system beep; no bundled assets).

use std::io::Write;

pub fn play_start() {
    if !crate::hotkey::current_settings().sound_on_start {
        return;
    }
    beep();
}

pub fn play_complete() {
    if !crate::hotkey::current_settings().sound_on_complete {
        return;
    }
    beep();
}

fn beep() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("afplay")
            .args(["/System/Library/Sounds/Tink.aiff"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        return;
    }
    #[cfg(target_os = "windows")]
    {
        // ASCII BEL — picked up by the console host / system speaker path.
        let _ = write!(std::io::stderr(), "\x07");
        let _ = std::io::stderr().flush();
    }
    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("paplay")
            .arg("/usr/share/sounds/freedesktop/stereo/message.oga")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
        {
            return;
        }
        let _ = write!(std::io::stderr(), "\x07");
        let _ = std::io::stderr().flush();
    }
}
