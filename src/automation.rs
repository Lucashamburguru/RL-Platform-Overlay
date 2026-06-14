pub async fn simulate_sequence(
    sequence: &str,
    action: &str,
    default_delay_ms: u64,
    system: &mut sysinfo::System,
) {
    let steps = parse_sequence(sequence, default_delay_ms);
    if steps.is_empty() {
        log::error!("{action} skipped: no valid steps in sequence '{sequence}'.");
        return;
    }

    for step in steps {
        match step {
            SequenceStep::Key(key) => {
                if !simulate_auto_key_tap(key, action, system).await {
                    return;
                }
            }
            SequenceStep::Delay(dur) => {
                tokio::time::sleep(dur).await;
            }
        }
    }
}

async fn simulate_key_tap(key: rdev::Key) -> Result<(), rdev::SimulateError> {
    rdev::simulate(&rdev::EventType::KeyPress(key))?;
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    rdev::simulate(&rdev::EventType::KeyRelease(key))?;
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    Ok(())
}

async fn simulate_auto_key_tap(key: rdev::Key, action: &str, system: &mut sysinfo::System) -> bool {
    if !rocket_league_accepts_auto_input(system) {
        log::info!("{action} skipped: Rocket League is not the foreground window.");
        return false;
    }

    if let Err(error) = simulate_key_tap(key).await {
        log::error!("{action} key simulation failed: {error:?}");
        return false;
    }

    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequenceStep {
    Key(rdev::Key),
    Delay(std::time::Duration),
}

pub(crate) fn parse_sequence(seq: &str, default_delay_ms: u64) -> Vec<SequenceStep> {
    let mut steps = Vec::new();
    let tokens = seq.split([',', ' ', '+']);
    for token in tokens {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let token_lower = token.to_lowercase();
        if token_lower.starts_with("delay") || token_lower.starts_with("wait") {
            let ms: u64 = token_lower
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(default_delay_ms);
            steps.push(SequenceStep::Delay(std::time::Duration::from_millis(ms)));
        } else if let Some(key) = parse_auto_key(token) {
            steps.push(SequenceStep::Key(key));
            if default_delay_ms > 0 {
                steps.push(SequenceStep::Delay(std::time::Duration::from_millis(
                    default_delay_ms,
                )));
            }
        }
    }
    steps
}

fn parse_auto_key(token: &str) -> Option<rdev::Key> {
    let mut normalized = token
        .trim()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase()
        .replace(['-', '_'], "");
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with("key") && normalized.len() == 4 {
        normalized = normalized[3..].to_string();
    }

    match normalized.as_str() {
        "enter" | "return" => Some(rdev::Key::Return),
        "escape" | "esc" => Some(rdev::Key::Escape),
        "space" => Some(rdev::Key::Space),
        "tab" => Some(rdev::Key::Tab),
        "backspace" => Some(rdev::Key::Backspace),
        "uparrow" | "up" => Some(rdev::Key::UpArrow),
        "downarrow" | "down" => Some(rdev::Key::DownArrow),
        "leftarrow" | "left" => Some(rdev::Key::LeftArrow),
        "rightarrow" | "right" => Some(rdev::Key::RightArrow),
        "0" | "num0" | "key0" => Some(rdev::Key::Num0),
        "1" | "num1" | "key1" => Some(rdev::Key::Num1),
        "2" | "num2" | "key2" => Some(rdev::Key::Num2),
        "3" | "num3" | "key3" => Some(rdev::Key::Num3),
        "4" | "num4" | "key4" => Some(rdev::Key::Num4),
        "5" | "num5" | "key5" => Some(rdev::Key::Num5),
        "6" | "num6" | "key6" => Some(rdev::Key::Num6),
        "7" | "num7" | "key7" => Some(rdev::Key::Num7),
        "8" | "num8" | "key8" => Some(rdev::Key::Num8),
        "9" | "num9" | "key9" => Some(rdev::Key::Num9),
        "kp0" | "numpad0" => Some(rdev::Key::Kp0),
        "kp1" | "numpad1" => Some(rdev::Key::Kp1),
        "kp2" | "numpad2" => Some(rdev::Key::Kp2),
        "kp3" | "numpad3" => Some(rdev::Key::Kp3),
        "kp4" | "numpad4" => Some(rdev::Key::Kp4),
        "kp5" | "numpad5" => Some(rdev::Key::Kp5),
        "kp6" | "numpad6" => Some(rdev::Key::Kp6),
        "kp7" | "numpad7" => Some(rdev::Key::Kp7),
        "kp8" | "numpad8" => Some(rdev::Key::Kp8),
        "kp9" | "numpad9" => Some(rdev::Key::Kp9),
        "kpenter" | "numpadenter" => Some(rdev::Key::KpReturn),
        letter if letter.len() == 1 => match letter.as_bytes()[0] {
            b'a' => Some(rdev::Key::KeyA),
            b'b' => Some(rdev::Key::KeyB),
            b'c' => Some(rdev::Key::KeyC),
            b'd' => Some(rdev::Key::KeyD),
            b'e' => Some(rdev::Key::KeyE),
            b'f' => Some(rdev::Key::KeyF),
            b'g' => Some(rdev::Key::KeyG),
            b'h' => Some(rdev::Key::KeyH),
            b'i' => Some(rdev::Key::KeyI),
            b'j' => Some(rdev::Key::KeyJ),
            b'k' => Some(rdev::Key::KeyK),
            b'l' => Some(rdev::Key::KeyL),
            b'm' => Some(rdev::Key::KeyM),
            b'n' => Some(rdev::Key::KeyN),
            b'o' => Some(rdev::Key::KeyO),
            b'p' => Some(rdev::Key::KeyP),
            b'q' => Some(rdev::Key::KeyQ),
            b'r' => Some(rdev::Key::KeyR),
            b's' => Some(rdev::Key::KeyS),
            b't' => Some(rdev::Key::KeyT),
            b'u' => Some(rdev::Key::KeyU),
            b'v' => Some(rdev::Key::KeyV),
            b'w' => Some(rdev::Key::KeyW),
            b'x' => Some(rdev::Key::KeyX),
            b'y' => Some(rdev::Key::KeyY),
            b'z' => Some(rdev::Key::KeyZ),
            _ => None,
        },
        _ => None,
    }
}

fn rocket_league_accepts_auto_input(system: &mut sysinfo::System) -> bool {
    #[cfg(target_os = "windows")]
    {
        is_rocket_league_foreground_window(system)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = system;
        true
    }
}

#[cfg(target_os = "windows")]
fn is_rocket_league_foreground_window(system: &mut sysinfo::System) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate};
    use winapi::um::winuser::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }

        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id == 0 {
            return false;
        }

        let pid = Pid::from_u32(process_id);
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

        system
            .process(pid)
            .is_some_and(|process| crate::assets::is_rocket_league_name(process.name()))
    }
}
