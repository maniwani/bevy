use bevy_ecs::entity::Entity;
use bevy_input::{
    keyboard::{KeyCode, KeyboardInput, NativeKeyCode},
    mouse::MouseButton,
    touch::{ForceTouch, TouchInput, TouchPhase},
    ButtonState,
};
use bevy_math::Vec2;
use bevy_window::{CursorIcon, EnabledButtons, WindowLevel, WindowTheme};
use winit::keyboard::{Key, NamedKey, NativeKey};

pub fn convert_keyboard_input(
    keyboard_input: &winit::event::KeyEvent,
    window: Entity,
) -> KeyboardInput {
    KeyboardInput {
        state: convert_element_state(keyboard_input.state),
        key_code: convert_physical_key_code(keyboard_input.physical_key),
        logical_key: convert_logical_key(&keyboard_input.logical_key),
        window,
    }
}

pub fn convert_element_state(element_state: winit::event::ElementState) -> ButtonState {
    match element_state {
        winit::event::ElementState::Pressed => ButtonState::Pressed,
        winit::event::ElementState::Released => ButtonState::Released,
    }
}

pub fn convert_mouse_button(mouse_button: winit::event::MouseButton) -> MouseButton {
    match mouse_button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(val) => MouseButton::Other(val),
    }
}

pub fn convert_touch_input(
    touch_input: winit::event::Touch,
    location: winit::dpi::LogicalPosition<f64>,
    window_entity: Entity,
) -> TouchInput {
    TouchInput {
        phase: match touch_input.phase {
            winit::event::TouchPhase::Started => TouchPhase::Started,
            winit::event::TouchPhase::Moved => TouchPhase::Moved,
            winit::event::TouchPhase::Ended => TouchPhase::Ended,
            winit::event::TouchPhase::Cancelled => TouchPhase::Canceled,
        },
        position: Vec2::new(location.x as f32, location.y as f32),
        window: window_entity,
        force: touch_input.force.map(|f| match f {
            winit::event::Force::Calibrated {
                force,
                max_possible_force,
                altitude_angle,
            } => ForceTouch::Calibrated {
                force,
                max_possible_force,
                altitude_angle,
            },
            winit::event::Force::Normalized(x) => ForceTouch::Normalized(x),
        }),
        id: touch_input.id,
    }
}

pub fn convert_physical_native_key_code(
    native_key_code: winit::keyboard::NativeKeyCode,
) -> NativeKeyCode {
    match native_key_code {
        winit::keyboard::NativeKeyCode::Unidentified => NativeKeyCode::Unidentified,
        winit::keyboard::NativeKeyCode::Android(scan_code) => NativeKeyCode::Android(scan_code),
        winit::keyboard::NativeKeyCode::MacOS(scan_code) => NativeKeyCode::MacOS(scan_code),
        winit::keyboard::NativeKeyCode::Windows(scan_code) => NativeKeyCode::Windows(scan_code),
        winit::keyboard::NativeKeyCode::Xkb(key_code) => NativeKeyCode::Xkb(key_code),
    }
}
pub fn convert_physical_key_code(virtual_key_code: winit::keyboard::PhysicalKey) -> KeyCode {
    match virtual_key_code {
        winit::keyboard::PhysicalKey::Unidentified(native_key_code) => {
            KeyCode::Unidentified(convert_physical_native_key_code(native_key_code))
        }
        winit::keyboard::PhysicalKey::Code(code) => match code {
            winit::keyboard::KeyCode::Backquote => KeyCode::Backquote,
            winit::keyboard::KeyCode::Backslash => KeyCode::Backslash,
            winit::keyboard::KeyCode::BracketLeft => KeyCode::BracketLeft,
            winit::keyboard::KeyCode::BracketRight => KeyCode::BracketRight,
            winit::keyboard::KeyCode::Comma => KeyCode::Comma,
            winit::keyboard::KeyCode::Digit0 => KeyCode::Digit0,
            winit::keyboard::KeyCode::Digit1 => KeyCode::Digit1,
            winit::keyboard::KeyCode::Digit2 => KeyCode::Digit2,
            winit::keyboard::KeyCode::Digit3 => KeyCode::Digit3,
            winit::keyboard::KeyCode::Digit4 => KeyCode::Digit4,
            winit::keyboard::KeyCode::Digit5 => KeyCode::Digit5,
            winit::keyboard::KeyCode::Digit6 => KeyCode::Digit6,
            winit::keyboard::KeyCode::Digit7 => KeyCode::Digit7,
            winit::keyboard::KeyCode::Digit8 => KeyCode::Digit8,
            winit::keyboard::KeyCode::Digit9 => KeyCode::Digit9,
            winit::keyboard::KeyCode::Equal => KeyCode::Equal,
            winit::keyboard::KeyCode::IntlBackslash => KeyCode::IntlBackslash,
            winit::keyboard::KeyCode::IntlRo => KeyCode::IntlRo,
            winit::keyboard::KeyCode::IntlYen => KeyCode::IntlYen,
            winit::keyboard::KeyCode::KeyA => KeyCode::KeyA,
            winit::keyboard::KeyCode::KeyB => KeyCode::KeyB,
            winit::keyboard::KeyCode::KeyC => KeyCode::KeyC,
            winit::keyboard::KeyCode::KeyD => KeyCode::KeyD,
            winit::keyboard::KeyCode::KeyE => KeyCode::KeyE,
            winit::keyboard::KeyCode::KeyF => KeyCode::KeyF,
            winit::keyboard::KeyCode::KeyG => KeyCode::KeyG,
            winit::keyboard::KeyCode::KeyH => KeyCode::KeyH,
            winit::keyboard::KeyCode::KeyI => KeyCode::KeyI,
            winit::keyboard::KeyCode::KeyJ => KeyCode::KeyJ,
            winit::keyboard::KeyCode::KeyK => KeyCode::KeyK,
            winit::keyboard::KeyCode::KeyL => KeyCode::KeyL,
            winit::keyboard::KeyCode::KeyM => KeyCode::KeyM,
            winit::keyboard::KeyCode::KeyN => KeyCode::KeyN,
            winit::keyboard::KeyCode::KeyO => KeyCode::KeyO,
            winit::keyboard::KeyCode::KeyP => KeyCode::KeyP,
            winit::keyboard::KeyCode::KeyQ => KeyCode::KeyQ,
            winit::keyboard::KeyCode::KeyR => KeyCode::KeyR,
            winit::keyboard::KeyCode::KeyS => KeyCode::KeyS,
            winit::keyboard::KeyCode::KeyT => KeyCode::KeyT,
            winit::keyboard::KeyCode::KeyU => KeyCode::KeyU,
            winit::keyboard::KeyCode::KeyV => KeyCode::KeyV,
            winit::keyboard::KeyCode::KeyW => KeyCode::KeyW,
            winit::keyboard::KeyCode::KeyX => KeyCode::KeyX,
            winit::keyboard::KeyCode::KeyY => KeyCode::KeyY,
            winit::keyboard::KeyCode::KeyZ => KeyCode::KeyZ,
            winit::keyboard::KeyCode::Minus => KeyCode::Minus,
            winit::keyboard::KeyCode::Period => KeyCode::Period,
            winit::keyboard::KeyCode::Quote => KeyCode::Quote,
            winit::keyboard::KeyCode::Semicolon => KeyCode::Semicolon,
            winit::keyboard::KeyCode::Slash => KeyCode::Slash,
            winit::keyboard::KeyCode::AltLeft => KeyCode::AltLeft,
            winit::keyboard::KeyCode::AltRight => KeyCode::AltRight,
            winit::keyboard::KeyCode::Backspace => KeyCode::Backspace,
            winit::keyboard::KeyCode::CapsLock => KeyCode::CapsLock,
            winit::keyboard::KeyCode::ContextMenu => KeyCode::ContextMenu,
            winit::keyboard::KeyCode::ControlLeft => KeyCode::ControlLeft,
            winit::keyboard::KeyCode::ControlRight => KeyCode::ControlRight,
            winit::keyboard::KeyCode::Enter => KeyCode::Enter,
            winit::keyboard::KeyCode::SuperLeft => KeyCode::SuperLeft,
            winit::keyboard::KeyCode::SuperRight => KeyCode::SuperRight,
            winit::keyboard::KeyCode::ShiftLeft => KeyCode::ShiftLeft,
            winit::keyboard::KeyCode::ShiftRight => KeyCode::ShiftRight,
            winit::keyboard::KeyCode::Space => KeyCode::Space,
            winit::keyboard::KeyCode::Tab => KeyCode::Tab,
            winit::keyboard::KeyCode::Convert => KeyCode::Convert,
            winit::keyboard::KeyCode::KanaMode => KeyCode::KanaMode,
            winit::keyboard::KeyCode::Lang1 => KeyCode::Lang1,
            winit::keyboard::KeyCode::Lang2 => KeyCode::Lang2,
            winit::keyboard::KeyCode::Lang3 => KeyCode::Lang3,
            winit::keyboard::KeyCode::Lang4 => KeyCode::Lang4,
            winit::keyboard::KeyCode::Lang5 => KeyCode::Lang5,
            winit::keyboard::KeyCode::NonConvert => KeyCode::NonConvert,
            winit::keyboard::KeyCode::Delete => KeyCode::Delete,
            winit::keyboard::KeyCode::End => KeyCode::End,
            winit::keyboard::KeyCode::Help => KeyCode::Help,
            winit::keyboard::KeyCode::Home => KeyCode::Home,
            winit::keyboard::KeyCode::Insert => KeyCode::Insert,
            winit::keyboard::KeyCode::PageDown => KeyCode::PageDown,
            winit::keyboard::KeyCode::PageUp => KeyCode::PageUp,
            winit::keyboard::KeyCode::ArrowDown => KeyCode::ArrowDown,
            winit::keyboard::KeyCode::ArrowLeft => KeyCode::ArrowLeft,
            winit::keyboard::KeyCode::ArrowRight => KeyCode::ArrowRight,
            winit::keyboard::KeyCode::ArrowUp => KeyCode::ArrowUp,
            winit::keyboard::KeyCode::NumLock => KeyCode::NumLock,
            winit::keyboard::KeyCode::Numpad0 => KeyCode::Numpad0,
            winit::keyboard::KeyCode::Numpad1 => KeyCode::Numpad1,
            winit::keyboard::KeyCode::Numpad2 => KeyCode::Numpad2,
            winit::keyboard::KeyCode::Numpad3 => KeyCode::Numpad3,
            winit::keyboard::KeyCode::Numpad4 => KeyCode::Numpad4,
            winit::keyboard::KeyCode::Numpad5 => KeyCode::Numpad5,
            winit::keyboard::KeyCode::Numpad6 => KeyCode::Numpad6,
            winit::keyboard::KeyCode::Numpad7 => KeyCode::Numpad7,
            winit::keyboard::KeyCode::Numpad8 => KeyCode::Numpad8,
            winit::keyboard::KeyCode::Numpad9 => KeyCode::Numpad9,
            winit::keyboard::KeyCode::NumpadAdd => KeyCode::NumpadAdd,
            winit::keyboard::KeyCode::NumpadBackspace => KeyCode::NumpadBackspace,
            winit::keyboard::KeyCode::NumpadClear => KeyCode::NumpadClear,
            winit::keyboard::KeyCode::NumpadClearEntry => KeyCode::NumpadClearEntry,
            winit::keyboard::KeyCode::NumpadComma => KeyCode::NumpadComma,
            winit::keyboard::KeyCode::NumpadDecimal => KeyCode::NumpadDecimal,
            winit::keyboard::KeyCode::NumpadDivide => KeyCode::NumpadDivide,
            winit::keyboard::KeyCode::NumpadEnter => KeyCode::NumpadEnter,
            winit::keyboard::KeyCode::NumpadEqual => KeyCode::NumpadEqual,
            winit::keyboard::KeyCode::NumpadHash => KeyCode::NumpadHash,
            winit::keyboard::KeyCode::NumpadMemoryAdd => KeyCode::NumpadMemoryAdd,
            winit::keyboard::KeyCode::NumpadMemoryClear => KeyCode::NumpadMemoryClear,
            winit::keyboard::KeyCode::NumpadMemoryRecall => KeyCode::NumpadMemoryRecall,
            winit::keyboard::KeyCode::NumpadMemoryStore => KeyCode::NumpadMemoryStore,
            winit::keyboard::KeyCode::NumpadMemorySubtract => KeyCode::NumpadMemorySubtract,
            winit::keyboard::KeyCode::NumpadMultiply => KeyCode::NumpadMultiply,
            winit::keyboard::KeyCode::NumpadParenLeft => KeyCode::NumpadParenLeft,
            winit::keyboard::KeyCode::NumpadParenRight => KeyCode::NumpadParenRight,
            winit::keyboard::KeyCode::NumpadStar => KeyCode::NumpadStar,
            winit::keyboard::KeyCode::NumpadSubtract => KeyCode::NumpadSubtract,
            winit::keyboard::KeyCode::Escape => KeyCode::Escape,
            winit::keyboard::KeyCode::Fn => KeyCode::Fn,
            winit::keyboard::KeyCode::FnLock => KeyCode::FnLock,
            winit::keyboard::KeyCode::PrintScreen => KeyCode::PrintScreen,
            winit::keyboard::KeyCode::ScrollLock => KeyCode::ScrollLock,
            winit::keyboard::KeyCode::Pause => KeyCode::Pause,
            winit::keyboard::KeyCode::BrowserBack => KeyCode::BrowserBack,
            winit::keyboard::KeyCode::BrowserFavorites => KeyCode::BrowserFavorites,
            winit::keyboard::KeyCode::BrowserForward => KeyCode::BrowserForward,
            winit::keyboard::KeyCode::BrowserHome => KeyCode::BrowserHome,
            winit::keyboard::KeyCode::BrowserRefresh => KeyCode::BrowserRefresh,
            winit::keyboard::KeyCode::BrowserSearch => KeyCode::BrowserSearch,
            winit::keyboard::KeyCode::BrowserStop => KeyCode::BrowserStop,
            winit::keyboard::KeyCode::Eject => KeyCode::Eject,
            winit::keyboard::KeyCode::LaunchApp1 => KeyCode::LaunchApp1,
            winit::keyboard::KeyCode::LaunchApp2 => KeyCode::LaunchApp2,
            winit::keyboard::KeyCode::LaunchMail => KeyCode::LaunchMail,
            winit::keyboard::KeyCode::MediaPlayPause => KeyCode::MediaPlayPause,
            winit::keyboard::KeyCode::MediaSelect => KeyCode::MediaSelect,
            winit::keyboard::KeyCode::MediaStop => KeyCode::MediaStop,
            winit::keyboard::KeyCode::MediaTrackNext => KeyCode::MediaTrackNext,
            winit::keyboard::KeyCode::MediaTrackPrevious => KeyCode::MediaTrackPrevious,
            winit::keyboard::KeyCode::Power => KeyCode::Power,
            winit::keyboard::KeyCode::Sleep => KeyCode::Sleep,
            winit::keyboard::KeyCode::AudioVolumeDown => KeyCode::AudioVolumeDown,
            winit::keyboard::KeyCode::AudioVolumeMute => KeyCode::AudioVolumeMute,
            winit::keyboard::KeyCode::AudioVolumeUp => KeyCode::AudioVolumeUp,
            winit::keyboard::KeyCode::WakeUp => KeyCode::WakeUp,
            winit::keyboard::KeyCode::Meta => KeyCode::Meta,
            winit::keyboard::KeyCode::Hyper => KeyCode::Hyper,
            winit::keyboard::KeyCode::Turbo => KeyCode::Turbo,
            winit::keyboard::KeyCode::Abort => KeyCode::Abort,
            winit::keyboard::KeyCode::Resume => KeyCode::Resume,
            winit::keyboard::KeyCode::Suspend => KeyCode::Suspend,
            winit::keyboard::KeyCode::Again => KeyCode::Again,
            winit::keyboard::KeyCode::Copy => KeyCode::Copy,
            winit::keyboard::KeyCode::Cut => KeyCode::Cut,
            winit::keyboard::KeyCode::Find => KeyCode::Find,
            winit::keyboard::KeyCode::Open => KeyCode::Open,
            winit::keyboard::KeyCode::Paste => KeyCode::Paste,
            winit::keyboard::KeyCode::Props => KeyCode::Props,
            winit::keyboard::KeyCode::Select => KeyCode::Select,
            winit::keyboard::KeyCode::Undo => KeyCode::Undo,
            winit::keyboard::KeyCode::Hiragana => KeyCode::Hiragana,
            winit::keyboard::KeyCode::Katakana => KeyCode::Katakana,
            winit::keyboard::KeyCode::F1 => KeyCode::F1,
            winit::keyboard::KeyCode::F2 => KeyCode::F2,
            winit::keyboard::KeyCode::F3 => KeyCode::F3,
            winit::keyboard::KeyCode::F4 => KeyCode::F4,
            winit::keyboard::KeyCode::F5 => KeyCode::F5,
            winit::keyboard::KeyCode::F6 => KeyCode::F6,
            winit::keyboard::KeyCode::F7 => KeyCode::F7,
            winit::keyboard::KeyCode::F8 => KeyCode::F8,
            winit::keyboard::KeyCode::F9 => KeyCode::F9,
            winit::keyboard::KeyCode::F10 => KeyCode::F10,
            winit::keyboard::KeyCode::F11 => KeyCode::F11,
            winit::keyboard::KeyCode::F12 => KeyCode::F12,
            winit::keyboard::KeyCode::F13 => KeyCode::F13,
            winit::keyboard::KeyCode::F14 => KeyCode::F14,
            winit::keyboard::KeyCode::F15 => KeyCode::F15,
            winit::keyboard::KeyCode::F16 => KeyCode::F16,
            winit::keyboard::KeyCode::F17 => KeyCode::F17,
            winit::keyboard::KeyCode::F18 => KeyCode::F18,
            winit::keyboard::KeyCode::F19 => KeyCode::F19,
            winit::keyboard::KeyCode::F20 => KeyCode::F20,
            winit::keyboard::KeyCode::F21 => KeyCode::F21,
            winit::keyboard::KeyCode::F22 => KeyCode::F22,
            winit::keyboard::KeyCode::F23 => KeyCode::F23,
            winit::keyboard::KeyCode::F24 => KeyCode::F24,
            winit::keyboard::KeyCode::F25 => KeyCode::F25,
            winit::keyboard::KeyCode::F26 => KeyCode::F26,
            winit::keyboard::KeyCode::F27 => KeyCode::F27,
            winit::keyboard::KeyCode::F28 => KeyCode::F28,
            winit::keyboard::KeyCode::F29 => KeyCode::F29,
            winit::keyboard::KeyCode::F30 => KeyCode::F30,
            winit::keyboard::KeyCode::F31 => KeyCode::F31,
            winit::keyboard::KeyCode::F32 => KeyCode::F32,
            winit::keyboard::KeyCode::F33 => KeyCode::F33,
            winit::keyboard::KeyCode::F34 => KeyCode::F34,
            winit::keyboard::KeyCode::F35 => KeyCode::F35,
            _ => KeyCode::Unidentified(NativeKeyCode::Unidentified),
        },
    }
}

pub fn convert_logical_key(logical_key_code: &winit::keyboard::Key) -> bevy_input::keyboard::Key {
    match logical_key_code {
        Key::Character(s) => bevy_input::keyboard::Key::Character(s.clone()),
        Key::Unidentified(nk) => bevy_input::keyboard::Key::Unidentified(convert_native_key(nk)),
        Key::Dead(c) => bevy_input::keyboard::Key::Dead(c.to_owned()),
        Key::Named(NamedKey::Alt) => bevy_input::keyboard::Key::Alt,
        Key::Named(NamedKey::AltGraph) => bevy_input::keyboard::Key::AltGraph,
        Key::Named(NamedKey::CapsLock) => bevy_input::keyboard::Key::CapsLock,
        Key::Named(NamedKey::Control) => bevy_input::keyboard::Key::Control,
        Key::Named(NamedKey::Fn) => bevy_input::keyboard::Key::Fn,
        Key::Named(NamedKey::FnLock) => bevy_input::keyboard::Key::FnLock,
        Key::Named(NamedKey::NumLock) => bevy_input::keyboard::Key::NumLock,
        Key::Named(NamedKey::ScrollLock) => bevy_input::keyboard::Key::ScrollLock,
        Key::Named(NamedKey::Shift) => bevy_input::keyboard::Key::Shift,
        Key::Named(NamedKey::Symbol) => bevy_input::keyboard::Key::Symbol,
        Key::Named(NamedKey::SymbolLock) => bevy_input::keyboard::Key::SymbolLock,
        Key::Named(NamedKey::Meta) => bevy_input::keyboard::Key::Meta,
        Key::Named(NamedKey::Hyper) => bevy_input::keyboard::Key::Hyper,
        Key::Named(NamedKey::Super) => bevy_input::keyboard::Key::Super,
        Key::Named(NamedKey::Enter) => bevy_input::keyboard::Key::Enter,
        Key::Named(NamedKey::Tab) => bevy_input::keyboard::Key::Tab,
        Key::Named(NamedKey::Space) => bevy_input::keyboard::Key::Space,
        Key::Named(NamedKey::ArrowDown) => bevy_input::keyboard::Key::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => bevy_input::keyboard::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => bevy_input::keyboard::Key::ArrowRight,
        Key::Named(NamedKey::ArrowUp) => bevy_input::keyboard::Key::ArrowUp,
        Key::Named(NamedKey::End) => bevy_input::keyboard::Key::End,
        Key::Named(NamedKey::Home) => bevy_input::keyboard::Key::Home,
        Key::Named(NamedKey::PageDown) => bevy_input::keyboard::Key::PageDown,
        Key::Named(NamedKey::PageUp) => bevy_input::keyboard::Key::PageUp,
        Key::Named(NamedKey::Backspace) => bevy_input::keyboard::Key::Backspace,
        Key::Named(NamedKey::Clear) => bevy_input::keyboard::Key::Clear,
        Key::Named(NamedKey::Copy) => bevy_input::keyboard::Key::Copy,
        Key::Named(NamedKey::CrSel) => bevy_input::keyboard::Key::CrSel,
        Key::Named(NamedKey::Cut) => bevy_input::keyboard::Key::Cut,
        Key::Named(NamedKey::Delete) => bevy_input::keyboard::Key::Delete,
        Key::Named(NamedKey::EraseEof) => bevy_input::keyboard::Key::EraseEof,
        Key::Named(NamedKey::ExSel) => bevy_input::keyboard::Key::ExSel,
        Key::Named(NamedKey::Insert) => bevy_input::keyboard::Key::Insert,
        Key::Named(NamedKey::Paste) => bevy_input::keyboard::Key::Paste,
        Key::Named(NamedKey::Redo) => bevy_input::keyboard::Key::Redo,
        Key::Named(NamedKey::Undo) => bevy_input::keyboard::Key::Undo,
        Key::Named(NamedKey::Accept) => bevy_input::keyboard::Key::Accept,
        Key::Named(NamedKey::Again) => bevy_input::keyboard::Key::Again,
        Key::Named(NamedKey::Attn) => bevy_input::keyboard::Key::Attn,
        Key::Named(NamedKey::Cancel) => bevy_input::keyboard::Key::Cancel,
        Key::Named(NamedKey::ContextMenu) => bevy_input::keyboard::Key::ContextMenu,
        Key::Named(NamedKey::Escape) => bevy_input::keyboard::Key::Escape,
        Key::Named(NamedKey::Execute) => bevy_input::keyboard::Key::Execute,
        Key::Named(NamedKey::Find) => bevy_input::keyboard::Key::Find,
        Key::Named(NamedKey::Help) => bevy_input::keyboard::Key::Help,
        Key::Named(NamedKey::Pause) => bevy_input::keyboard::Key::Pause,
        Key::Named(NamedKey::Play) => bevy_input::keyboard::Key::Play,
        Key::Named(NamedKey::Props) => bevy_input::keyboard::Key::Props,
        Key::Named(NamedKey::Select) => bevy_input::keyboard::Key::Select,
        Key::Named(NamedKey::ZoomIn) => bevy_input::keyboard::Key::ZoomIn,
        Key::Named(NamedKey::ZoomOut) => bevy_input::keyboard::Key::ZoomOut,
        Key::Named(NamedKey::BrightnessDown) => bevy_input::keyboard::Key::BrightnessDown,
        Key::Named(NamedKey::BrightnessUp) => bevy_input::keyboard::Key::BrightnessUp,
        Key::Named(NamedKey::Eject) => bevy_input::keyboard::Key::Eject,
        Key::Named(NamedKey::LogOff) => bevy_input::keyboard::Key::LogOff,
        Key::Named(NamedKey::Power) => bevy_input::keyboard::Key::Power,
        Key::Named(NamedKey::PowerOff) => bevy_input::keyboard::Key::PowerOff,
        Key::Named(NamedKey::PrintScreen) => bevy_input::keyboard::Key::PrintScreen,
        Key::Named(NamedKey::Hibernate) => bevy_input::keyboard::Key::Hibernate,
        Key::Named(NamedKey::Standby) => bevy_input::keyboard::Key::Standby,
        Key::Named(NamedKey::WakeUp) => bevy_input::keyboard::Key::WakeUp,
        Key::Named(NamedKey::AllCandidates) => bevy_input::keyboard::Key::AllCandidates,
        Key::Named(NamedKey::Alphanumeric) => bevy_input::keyboard::Key::Alphanumeric,
        Key::Named(NamedKey::CodeInput) => bevy_input::keyboard::Key::CodeInput,
        Key::Named(NamedKey::Compose) => bevy_input::keyboard::Key::Compose,
        Key::Named(NamedKey::Convert) => bevy_input::keyboard::Key::Convert,
        Key::Named(NamedKey::FinalMode) => bevy_input::keyboard::Key::FinalMode,
        Key::Named(NamedKey::GroupFirst) => bevy_input::keyboard::Key::GroupFirst,
        Key::Named(NamedKey::GroupLast) => bevy_input::keyboard::Key::GroupLast,
        Key::Named(NamedKey::GroupNext) => bevy_input::keyboard::Key::GroupNext,
        Key::Named(NamedKey::GroupPrevious) => bevy_input::keyboard::Key::GroupPrevious,
        Key::Named(NamedKey::ModeChange) => bevy_input::keyboard::Key::ModeChange,
        Key::Named(NamedKey::NextCandidate) => bevy_input::keyboard::Key::NextCandidate,
        Key::Named(NamedKey::NonConvert) => bevy_input::keyboard::Key::NonConvert,
        Key::Named(NamedKey::PreviousCandidate) => bevy_input::keyboard::Key::PreviousCandidate,
        Key::Named(NamedKey::Process) => bevy_input::keyboard::Key::Process,
        Key::Named(NamedKey::SingleCandidate) => bevy_input::keyboard::Key::SingleCandidate,
        Key::Named(NamedKey::HangulMode) => bevy_input::keyboard::Key::HangulMode,
        Key::Named(NamedKey::HanjaMode) => bevy_input::keyboard::Key::HanjaMode,
        Key::Named(NamedKey::JunjaMode) => bevy_input::keyboard::Key::JunjaMode,
        Key::Named(NamedKey::Eisu) => bevy_input::keyboard::Key::Eisu,
        Key::Named(NamedKey::Hankaku) => bevy_input::keyboard::Key::Hankaku,
        Key::Named(NamedKey::Hiragana) => bevy_input::keyboard::Key::Hiragana,
        Key::Named(NamedKey::HiraganaKatakana) => bevy_input::keyboard::Key::HiraganaKatakana,
        Key::Named(NamedKey::KanaMode) => bevy_input::keyboard::Key::KanaMode,
        Key::Named(NamedKey::KanjiMode) => bevy_input::keyboard::Key::KanjiMode,
        Key::Named(NamedKey::Katakana) => bevy_input::keyboard::Key::Katakana,
        Key::Named(NamedKey::Romaji) => bevy_input::keyboard::Key::Romaji,
        Key::Named(NamedKey::Zenkaku) => bevy_input::keyboard::Key::Zenkaku,
        Key::Named(NamedKey::ZenkakuHankaku) => bevy_input::keyboard::Key::ZenkakuHankaku,
        Key::Named(NamedKey::Soft1) => bevy_input::keyboard::Key::Soft1,
        Key::Named(NamedKey::Soft2) => bevy_input::keyboard::Key::Soft2,
        Key::Named(NamedKey::Soft3) => bevy_input::keyboard::Key::Soft3,
        Key::Named(NamedKey::Soft4) => bevy_input::keyboard::Key::Soft4,
        Key::Named(NamedKey::ChannelDown) => bevy_input::keyboard::Key::ChannelDown,
        Key::Named(NamedKey::ChannelUp) => bevy_input::keyboard::Key::ChannelUp,
        Key::Named(NamedKey::Close) => bevy_input::keyboard::Key::Close,
        Key::Named(NamedKey::MailForward) => bevy_input::keyboard::Key::MailForward,
        Key::Named(NamedKey::MailReply) => bevy_input::keyboard::Key::MailReply,
        Key::Named(NamedKey::MailSend) => bevy_input::keyboard::Key::MailSend,
        Key::Named(NamedKey::MediaClose) => bevy_input::keyboard::Key::MediaClose,
        Key::Named(NamedKey::MediaFastForward) => bevy_input::keyboard::Key::MediaFastForward,
        Key::Named(NamedKey::MediaPause) => bevy_input::keyboard::Key::MediaPause,
        Key::Named(NamedKey::MediaPlay) => bevy_input::keyboard::Key::MediaPlay,
        Key::Named(NamedKey::MediaPlayPause) => bevy_input::keyboard::Key::MediaPlayPause,
        Key::Named(NamedKey::MediaRecord) => bevy_input::keyboard::Key::MediaRecord,
        Key::Named(NamedKey::MediaRewind) => bevy_input::keyboard::Key::MediaRewind,
        Key::Named(NamedKey::MediaStop) => bevy_input::keyboard::Key::MediaStop,
        Key::Named(NamedKey::MediaTrackNext) => bevy_input::keyboard::Key::MediaTrackNext,
        Key::Named(NamedKey::MediaTrackPrevious) => bevy_input::keyboard::Key::MediaTrackPrevious,
        Key::Named(NamedKey::New) => bevy_input::keyboard::Key::New,
        Key::Named(NamedKey::Open) => bevy_input::keyboard::Key::Open,
        Key::Named(NamedKey::Print) => bevy_input::keyboard::Key::Print,
        Key::Named(NamedKey::Save) => bevy_input::keyboard::Key::Save,
        Key::Named(NamedKey::SpellCheck) => bevy_input::keyboard::Key::SpellCheck,
        Key::Named(NamedKey::Key11) => bevy_input::keyboard::Key::Key11,
        Key::Named(NamedKey::Key12) => bevy_input::keyboard::Key::Key12,
        Key::Named(NamedKey::AudioBalanceLeft) => bevy_input::keyboard::Key::AudioBalanceLeft,
        Key::Named(NamedKey::AudioBalanceRight) => bevy_input::keyboard::Key::AudioBalanceRight,
        Key::Named(NamedKey::AudioBassBoostDown) => bevy_input::keyboard::Key::AudioBassBoostDown,
        Key::Named(NamedKey::AudioBassBoostToggle) => {
            bevy_input::keyboard::Key::AudioBassBoostToggle
        }
        Key::Named(NamedKey::AudioBassBoostUp) => bevy_input::keyboard::Key::AudioBassBoostUp,
        Key::Named(NamedKey::AudioFaderFront) => bevy_input::keyboard::Key::AudioFaderFront,
        Key::Named(NamedKey::AudioFaderRear) => bevy_input::keyboard::Key::AudioFaderRear,
        Key::Named(NamedKey::AudioSurroundModeNext) => {
            bevy_input::keyboard::Key::AudioSurroundModeNext
        }
        Key::Named(NamedKey::AudioTrebleDown) => bevy_input::keyboard::Key::AudioTrebleDown,
        Key::Named(NamedKey::AudioTrebleUp) => bevy_input::keyboard::Key::AudioTrebleUp,
        Key::Named(NamedKey::AudioVolumeDown) => bevy_input::keyboard::Key::AudioVolumeDown,
        Key::Named(NamedKey::AudioVolumeUp) => bevy_input::keyboard::Key::AudioVolumeUp,
        Key::Named(NamedKey::AudioVolumeMute) => bevy_input::keyboard::Key::AudioVolumeMute,
        Key::Named(NamedKey::MicrophoneToggle) => bevy_input::keyboard::Key::MicrophoneToggle,
        Key::Named(NamedKey::MicrophoneVolumeDown) => {
            bevy_input::keyboard::Key::MicrophoneVolumeDown
        }
        Key::Named(NamedKey::MicrophoneVolumeUp) => bevy_input::keyboard::Key::MicrophoneVolumeUp,
        Key::Named(NamedKey::MicrophoneVolumeMute) => {
            bevy_input::keyboard::Key::MicrophoneVolumeMute
        }
        Key::Named(NamedKey::SpeechCorrectionList) => {
            bevy_input::keyboard::Key::SpeechCorrectionList
        }
        Key::Named(NamedKey::SpeechInputToggle) => bevy_input::keyboard::Key::SpeechInputToggle,
        Key::Named(NamedKey::LaunchApplication1) => bevy_input::keyboard::Key::LaunchApplication1,
        Key::Named(NamedKey::LaunchApplication2) => bevy_input::keyboard::Key::LaunchApplication2,
        Key::Named(NamedKey::LaunchCalendar) => bevy_input::keyboard::Key::LaunchCalendar,
        Key::Named(NamedKey::LaunchContacts) => bevy_input::keyboard::Key::LaunchContacts,
        Key::Named(NamedKey::LaunchMail) => bevy_input::keyboard::Key::LaunchMail,
        Key::Named(NamedKey::LaunchMediaPlayer) => bevy_input::keyboard::Key::LaunchMediaPlayer,
        Key::Named(NamedKey::LaunchMusicPlayer) => bevy_input::keyboard::Key::LaunchMusicPlayer,
        Key::Named(NamedKey::LaunchPhone) => bevy_input::keyboard::Key::LaunchPhone,
        Key::Named(NamedKey::LaunchScreenSaver) => bevy_input::keyboard::Key::LaunchScreenSaver,
        Key::Named(NamedKey::LaunchSpreadsheet) => bevy_input::keyboard::Key::LaunchSpreadsheet,
        Key::Named(NamedKey::LaunchWebBrowser) => bevy_input::keyboard::Key::LaunchWebBrowser,
        Key::Named(NamedKey::LaunchWebCam) => bevy_input::keyboard::Key::LaunchWebCam,
        Key::Named(NamedKey::LaunchWordProcessor) => bevy_input::keyboard::Key::LaunchWordProcessor,
        Key::Named(NamedKey::BrowserBack) => bevy_input::keyboard::Key::BrowserBack,
        Key::Named(NamedKey::BrowserFavorites) => bevy_input::keyboard::Key::BrowserFavorites,
        Key::Named(NamedKey::BrowserForward) => bevy_input::keyboard::Key::BrowserForward,
        Key::Named(NamedKey::BrowserHome) => bevy_input::keyboard::Key::BrowserHome,
        Key::Named(NamedKey::BrowserRefresh) => bevy_input::keyboard::Key::BrowserRefresh,
        Key::Named(NamedKey::BrowserSearch) => bevy_input::keyboard::Key::BrowserSearch,
        Key::Named(NamedKey::BrowserStop) => bevy_input::keyboard::Key::BrowserStop,
        Key::Named(NamedKey::AppSwitch) => bevy_input::keyboard::Key::AppSwitch,
        Key::Named(NamedKey::Call) => bevy_input::keyboard::Key::Call,
        Key::Named(NamedKey::Camera) => bevy_input::keyboard::Key::Camera,
        Key::Named(NamedKey::CameraFocus) => bevy_input::keyboard::Key::CameraFocus,
        Key::Named(NamedKey::EndCall) => bevy_input::keyboard::Key::EndCall,
        Key::Named(NamedKey::GoBack) => bevy_input::keyboard::Key::GoBack,
        Key::Named(NamedKey::GoHome) => bevy_input::keyboard::Key::GoHome,
        Key::Named(NamedKey::HeadsetHook) => bevy_input::keyboard::Key::HeadsetHook,
        Key::Named(NamedKey::LastNumberRedial) => bevy_input::keyboard::Key::LastNumberRedial,
        Key::Named(NamedKey::Notification) => bevy_input::keyboard::Key::Notification,
        Key::Named(NamedKey::MannerMode) => bevy_input::keyboard::Key::MannerMode,
        Key::Named(NamedKey::VoiceDial) => bevy_input::keyboard::Key::VoiceDial,
        Key::Named(NamedKey::TV) => bevy_input::keyboard::Key::TV,
        Key::Named(NamedKey::TV3DMode) => bevy_input::keyboard::Key::TV3DMode,
        Key::Named(NamedKey::TVAntennaCable) => bevy_input::keyboard::Key::TVAntennaCable,
        Key::Named(NamedKey::TVAudioDescription) => bevy_input::keyboard::Key::TVAudioDescription,
        Key::Named(NamedKey::TVAudioDescriptionMixDown) => {
            bevy_input::keyboard::Key::TVAudioDescriptionMixDown
        }
        Key::Named(NamedKey::TVAudioDescriptionMixUp) => {
            bevy_input::keyboard::Key::TVAudioDescriptionMixUp
        }
        Key::Named(NamedKey::TVContentsMenu) => bevy_input::keyboard::Key::TVContentsMenu,
        Key::Named(NamedKey::TVDataService) => bevy_input::keyboard::Key::TVDataService,
        Key::Named(NamedKey::TVInput) => bevy_input::keyboard::Key::TVInput,
        Key::Named(NamedKey::TVInputComponent1) => bevy_input::keyboard::Key::TVInputComponent1,
        Key::Named(NamedKey::TVInputComponent2) => bevy_input::keyboard::Key::TVInputComponent2,
        Key::Named(NamedKey::TVInputComposite1) => bevy_input::keyboard::Key::TVInputComposite1,
        Key::Named(NamedKey::TVInputComposite2) => bevy_input::keyboard::Key::TVInputComposite2,
        Key::Named(NamedKey::TVInputHDMI1) => bevy_input::keyboard::Key::TVInputHDMI1,
        Key::Named(NamedKey::TVInputHDMI2) => bevy_input::keyboard::Key::TVInputHDMI2,
        Key::Named(NamedKey::TVInputHDMI3) => bevy_input::keyboard::Key::TVInputHDMI3,
        Key::Named(NamedKey::TVInputHDMI4) => bevy_input::keyboard::Key::TVInputHDMI4,
        Key::Named(NamedKey::TVInputVGA1) => bevy_input::keyboard::Key::TVInputVGA1,
        Key::Named(NamedKey::TVMediaContext) => bevy_input::keyboard::Key::TVMediaContext,
        Key::Named(NamedKey::TVNetwork) => bevy_input::keyboard::Key::TVNetwork,
        Key::Named(NamedKey::TVNumberEntry) => bevy_input::keyboard::Key::TVNumberEntry,
        Key::Named(NamedKey::TVPower) => bevy_input::keyboard::Key::TVPower,
        Key::Named(NamedKey::TVRadioService) => bevy_input::keyboard::Key::TVRadioService,
        Key::Named(NamedKey::TVSatellite) => bevy_input::keyboard::Key::TVSatellite,
        Key::Named(NamedKey::TVSatelliteBS) => bevy_input::keyboard::Key::TVSatelliteBS,
        Key::Named(NamedKey::TVSatelliteCS) => bevy_input::keyboard::Key::TVSatelliteCS,
        Key::Named(NamedKey::TVSatelliteToggle) => bevy_input::keyboard::Key::TVSatelliteToggle,
        Key::Named(NamedKey::TVTerrestrialAnalog) => bevy_input::keyboard::Key::TVTerrestrialAnalog,
        Key::Named(NamedKey::TVTerrestrialDigital) => {
            bevy_input::keyboard::Key::TVTerrestrialDigital
        }
        Key::Named(NamedKey::TVTimer) => bevy_input::keyboard::Key::TVTimer,
        Key::Named(NamedKey::AVRInput) => bevy_input::keyboard::Key::AVRInput,
        Key::Named(NamedKey::AVRPower) => bevy_input::keyboard::Key::AVRPower,
        Key::Named(NamedKey::ColorF0Red) => bevy_input::keyboard::Key::ColorF0Red,
        Key::Named(NamedKey::ColorF1Green) => bevy_input::keyboard::Key::ColorF1Green,
        Key::Named(NamedKey::ColorF2Yellow) => bevy_input::keyboard::Key::ColorF2Yellow,
        Key::Named(NamedKey::ColorF3Blue) => bevy_input::keyboard::Key::ColorF3Blue,
        Key::Named(NamedKey::ColorF4Grey) => bevy_input::keyboard::Key::ColorF4Grey,
        Key::Named(NamedKey::ColorF5Brown) => bevy_input::keyboard::Key::ColorF5Brown,
        Key::Named(NamedKey::ClosedCaptionToggle) => bevy_input::keyboard::Key::ClosedCaptionToggle,
        Key::Named(NamedKey::Dimmer) => bevy_input::keyboard::Key::Dimmer,
        Key::Named(NamedKey::DisplaySwap) => bevy_input::keyboard::Key::DisplaySwap,
        Key::Named(NamedKey::DVR) => bevy_input::keyboard::Key::DVR,
        Key::Named(NamedKey::Exit) => bevy_input::keyboard::Key::Exit,
        Key::Named(NamedKey::FavoriteClear0) => bevy_input::keyboard::Key::FavoriteClear0,
        Key::Named(NamedKey::FavoriteClear1) => bevy_input::keyboard::Key::FavoriteClear1,
        Key::Named(NamedKey::FavoriteClear2) => bevy_input::keyboard::Key::FavoriteClear2,
        Key::Named(NamedKey::FavoriteClear3) => bevy_input::keyboard::Key::FavoriteClear3,
        Key::Named(NamedKey::FavoriteRecall0) => bevy_input::keyboard::Key::FavoriteRecall0,
        Key::Named(NamedKey::FavoriteRecall1) => bevy_input::keyboard::Key::FavoriteRecall1,
        Key::Named(NamedKey::FavoriteRecall2) => bevy_input::keyboard::Key::FavoriteRecall2,
        Key::Named(NamedKey::FavoriteRecall3) => bevy_input::keyboard::Key::FavoriteRecall3,
        Key::Named(NamedKey::FavoriteStore0) => bevy_input::keyboard::Key::FavoriteStore0,
        Key::Named(NamedKey::FavoriteStore1) => bevy_input::keyboard::Key::FavoriteStore1,
        Key::Named(NamedKey::FavoriteStore2) => bevy_input::keyboard::Key::FavoriteStore2,
        Key::Named(NamedKey::FavoriteStore3) => bevy_input::keyboard::Key::FavoriteStore3,
        Key::Named(NamedKey::Guide) => bevy_input::keyboard::Key::Guide,
        Key::Named(NamedKey::GuideNextDay) => bevy_input::keyboard::Key::GuideNextDay,
        Key::Named(NamedKey::GuidePreviousDay) => bevy_input::keyboard::Key::GuidePreviousDay,
        Key::Named(NamedKey::Info) => bevy_input::keyboard::Key::Info,
        Key::Named(NamedKey::InstantReplay) => bevy_input::keyboard::Key::InstantReplay,
        Key::Named(NamedKey::Link) => bevy_input::keyboard::Key::Link,
        Key::Named(NamedKey::ListProgram) => bevy_input::keyboard::Key::ListProgram,
        Key::Named(NamedKey::LiveContent) => bevy_input::keyboard::Key::LiveContent,
        Key::Named(NamedKey::Lock) => bevy_input::keyboard::Key::Lock,
        Key::Named(NamedKey::MediaApps) => bevy_input::keyboard::Key::MediaApps,
        Key::Named(NamedKey::MediaAudioTrack) => bevy_input::keyboard::Key::MediaAudioTrack,
        Key::Named(NamedKey::MediaLast) => bevy_input::keyboard::Key::MediaLast,
        Key::Named(NamedKey::MediaSkipBackward) => bevy_input::keyboard::Key::MediaSkipBackward,
        Key::Named(NamedKey::MediaSkipForward) => bevy_input::keyboard::Key::MediaSkipForward,
        Key::Named(NamedKey::MediaStepBackward) => bevy_input::keyboard::Key::MediaStepBackward,
        Key::Named(NamedKey::MediaStepForward) => bevy_input::keyboard::Key::MediaStepForward,
        Key::Named(NamedKey::MediaTopMenu) => bevy_input::keyboard::Key::MediaTopMenu,
        Key::Named(NamedKey::NavigateIn) => bevy_input::keyboard::Key::NavigateIn,
        Key::Named(NamedKey::NavigateNext) => bevy_input::keyboard::Key::NavigateNext,
        Key::Named(NamedKey::NavigateOut) => bevy_input::keyboard::Key::NavigateOut,
        Key::Named(NamedKey::NavigatePrevious) => bevy_input::keyboard::Key::NavigatePrevious,
        Key::Named(NamedKey::NextFavoriteChannel) => bevy_input::keyboard::Key::NextFavoriteChannel,
        Key::Named(NamedKey::NextUserProfile) => bevy_input::keyboard::Key::NextUserProfile,
        Key::Named(NamedKey::OnDemand) => bevy_input::keyboard::Key::OnDemand,
        Key::Named(NamedKey::Pairing) => bevy_input::keyboard::Key::Pairing,
        Key::Named(NamedKey::PinPDown) => bevy_input::keyboard::Key::PinPDown,
        Key::Named(NamedKey::PinPMove) => bevy_input::keyboard::Key::PinPMove,
        Key::Named(NamedKey::PinPToggle) => bevy_input::keyboard::Key::PinPToggle,
        Key::Named(NamedKey::PinPUp) => bevy_input::keyboard::Key::PinPUp,
        Key::Named(NamedKey::PlaySpeedDown) => bevy_input::keyboard::Key::PlaySpeedDown,
        Key::Named(NamedKey::PlaySpeedReset) => bevy_input::keyboard::Key::PlaySpeedReset,
        Key::Named(NamedKey::PlaySpeedUp) => bevy_input::keyboard::Key::PlaySpeedUp,
        Key::Named(NamedKey::RandomToggle) => bevy_input::keyboard::Key::RandomToggle,
        Key::Named(NamedKey::RcLowBattery) => bevy_input::keyboard::Key::RcLowBattery,
        Key::Named(NamedKey::RecordSpeedNext) => bevy_input::keyboard::Key::RecordSpeedNext,
        Key::Named(NamedKey::RfBypass) => bevy_input::keyboard::Key::RfBypass,
        Key::Named(NamedKey::ScanChannelsToggle) => bevy_input::keyboard::Key::ScanChannelsToggle,
        Key::Named(NamedKey::ScreenModeNext) => bevy_input::keyboard::Key::ScreenModeNext,
        Key::Named(NamedKey::Settings) => bevy_input::keyboard::Key::Settings,
        Key::Named(NamedKey::SplitScreenToggle) => bevy_input::keyboard::Key::SplitScreenToggle,
        Key::Named(NamedKey::STBInput) => bevy_input::keyboard::Key::STBInput,
        Key::Named(NamedKey::STBPower) => bevy_input::keyboard::Key::STBPower,
        Key::Named(NamedKey::Subtitle) => bevy_input::keyboard::Key::Subtitle,
        Key::Named(NamedKey::Teletext) => bevy_input::keyboard::Key::Teletext,
        Key::Named(NamedKey::VideoModeNext) => bevy_input::keyboard::Key::VideoModeNext,
        Key::Named(NamedKey::Wink) => bevy_input::keyboard::Key::Wink,
        Key::Named(NamedKey::ZoomToggle) => bevy_input::keyboard::Key::ZoomToggle,
        Key::Named(NamedKey::F1) => bevy_input::keyboard::Key::F1,
        Key::Named(NamedKey::F2) => bevy_input::keyboard::Key::F2,
        Key::Named(NamedKey::F3) => bevy_input::keyboard::Key::F3,
        Key::Named(NamedKey::F4) => bevy_input::keyboard::Key::F4,
        Key::Named(NamedKey::F5) => bevy_input::keyboard::Key::F5,
        Key::Named(NamedKey::F6) => bevy_input::keyboard::Key::F6,
        Key::Named(NamedKey::F7) => bevy_input::keyboard::Key::F7,
        Key::Named(NamedKey::F8) => bevy_input::keyboard::Key::F8,
        Key::Named(NamedKey::F9) => bevy_input::keyboard::Key::F9,
        Key::Named(NamedKey::F10) => bevy_input::keyboard::Key::F10,
        Key::Named(NamedKey::F11) => bevy_input::keyboard::Key::F11,
        Key::Named(NamedKey::F12) => bevy_input::keyboard::Key::F12,
        Key::Named(NamedKey::F13) => bevy_input::keyboard::Key::F13,
        Key::Named(NamedKey::F14) => bevy_input::keyboard::Key::F14,
        Key::Named(NamedKey::F15) => bevy_input::keyboard::Key::F15,
        Key::Named(NamedKey::F16) => bevy_input::keyboard::Key::F16,
        Key::Named(NamedKey::F17) => bevy_input::keyboard::Key::F17,
        Key::Named(NamedKey::F18) => bevy_input::keyboard::Key::F18,
        Key::Named(NamedKey::F19) => bevy_input::keyboard::Key::F19,
        Key::Named(NamedKey::F20) => bevy_input::keyboard::Key::F20,
        Key::Named(NamedKey::F21) => bevy_input::keyboard::Key::F21,
        Key::Named(NamedKey::F22) => bevy_input::keyboard::Key::F22,
        Key::Named(NamedKey::F23) => bevy_input::keyboard::Key::F23,
        Key::Named(NamedKey::F24) => bevy_input::keyboard::Key::F24,
        Key::Named(NamedKey::F25) => bevy_input::keyboard::Key::F25,
        Key::Named(NamedKey::F26) => bevy_input::keyboard::Key::F26,
        Key::Named(NamedKey::F27) => bevy_input::keyboard::Key::F27,
        Key::Named(NamedKey::F28) => bevy_input::keyboard::Key::F28,
        Key::Named(NamedKey::F29) => bevy_input::keyboard::Key::F29,
        Key::Named(NamedKey::F30) => bevy_input::keyboard::Key::F30,
        Key::Named(NamedKey::F31) => bevy_input::keyboard::Key::F31,
        Key::Named(NamedKey::F32) => bevy_input::keyboard::Key::F32,
        Key::Named(NamedKey::F33) => bevy_input::keyboard::Key::F33,
        Key::Named(NamedKey::F34) => bevy_input::keyboard::Key::F34,
        Key::Named(NamedKey::F35) => bevy_input::keyboard::Key::F35,
        _ => todo!(),
    }
}

pub fn convert_native_key(native_key: &NativeKey) -> bevy_input::keyboard::NativeKey {
    match native_key {
        NativeKey::Unidentified => bevy_input::keyboard::NativeKey::Unidentified,
        NativeKey::Android(v) => bevy_input::keyboard::NativeKey::Android(*v),
        NativeKey::MacOS(v) => bevy_input::keyboard::NativeKey::MacOS(*v),
        NativeKey::Windows(v) => bevy_input::keyboard::NativeKey::Windows(*v),
        NativeKey::Xkb(v) => bevy_input::keyboard::NativeKey::Xkb(*v),
        NativeKey::Web(v) => bevy_input::keyboard::NativeKey::Web(v.clone()),
    }
}

pub fn convert_cursor_icon(cursor_icon: CursorIcon) -> winit::window::CursorIcon {
    match cursor_icon {
        CursorIcon::Crosshair => winit::window::CursorIcon::Crosshair,
        CursorIcon::Pointer => winit::window::CursorIcon::Pointer,
        CursorIcon::Move => winit::window::CursorIcon::Move,
        CursorIcon::Text => winit::window::CursorIcon::Text,
        CursorIcon::Wait => winit::window::CursorIcon::Wait,
        CursorIcon::Help => winit::window::CursorIcon::Help,
        CursorIcon::Progress => winit::window::CursorIcon::Progress,
        CursorIcon::NotAllowed => winit::window::CursorIcon::NotAllowed,
        CursorIcon::ContextMenu => winit::window::CursorIcon::ContextMenu,
        CursorIcon::Cell => winit::window::CursorIcon::Cell,
        CursorIcon::VerticalText => winit::window::CursorIcon::VerticalText,
        CursorIcon::Alias => winit::window::CursorIcon::Alias,
        CursorIcon::Copy => winit::window::CursorIcon::Copy,
        CursorIcon::NoDrop => winit::window::CursorIcon::NoDrop,
        CursorIcon::Grab => winit::window::CursorIcon::Grab,
        CursorIcon::Grabbing => winit::window::CursorIcon::Grabbing,
        CursorIcon::AllScroll => winit::window::CursorIcon::AllScroll,
        CursorIcon::ZoomIn => winit::window::CursorIcon::ZoomIn,
        CursorIcon::ZoomOut => winit::window::CursorIcon::ZoomOut,
        CursorIcon::EResize => winit::window::CursorIcon::EResize,
        CursorIcon::NResize => winit::window::CursorIcon::NResize,
        CursorIcon::NeResize => winit::window::CursorIcon::NeResize,
        CursorIcon::NwResize => winit::window::CursorIcon::NwResize,
        CursorIcon::SResize => winit::window::CursorIcon::SResize,
        CursorIcon::SeResize => winit::window::CursorIcon::SeResize,
        CursorIcon::SwResize => winit::window::CursorIcon::SwResize,
        CursorIcon::WResize => winit::window::CursorIcon::WResize,
        CursorIcon::EwResize => winit::window::CursorIcon::EwResize,
        CursorIcon::NsResize => winit::window::CursorIcon::NsResize,
        CursorIcon::NeswResize => winit::window::CursorIcon::NeswResize,
        CursorIcon::NwseResize => winit::window::CursorIcon::NwseResize,
        CursorIcon::ColResize => winit::window::CursorIcon::ColResize,
        CursorIcon::RowResize => winit::window::CursorIcon::RowResize,
        _ => winit::window::CursorIcon::Default,
    }
}

pub fn convert_window_level(window_level: WindowLevel) -> winit::window::WindowLevel {
    match window_level {
        WindowLevel::AlwaysOnBottom => winit::window::WindowLevel::AlwaysOnBottom,
        WindowLevel::Normal => winit::window::WindowLevel::Normal,
        WindowLevel::AlwaysOnTop => winit::window::WindowLevel::AlwaysOnTop,
    }
}

pub fn convert_winit_theme(theme: winit::window::Theme) -> WindowTheme {
    match theme {
        winit::window::Theme::Light => WindowTheme::Light,
        winit::window::Theme::Dark => WindowTheme::Dark,
    }
}

pub fn convert_window_theme(theme: WindowTheme) -> winit::window::Theme {
    match theme {
        WindowTheme::Light => winit::window::Theme::Light,
        WindowTheme::Dark => winit::window::Theme::Dark,
    }
}

pub fn convert_enabled_buttons(enabled_buttons: EnabledButtons) -> winit::window::WindowButtons {
    let mut window_buttons = winit::window::WindowButtons::empty();
    if enabled_buttons.minimize {
        window_buttons.insert(winit::window::WindowButtons::MINIMIZE);
    }
    if enabled_buttons.maximize {
        window_buttons.insert(winit::window::WindowButtons::MAXIMIZE);
    }
    if enabled_buttons.close {
        window_buttons.insert(winit::window::WindowButtons::CLOSE);
    }
    window_buttons
}

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use winit::dpi::{PhysicalPosition, PhysicalSize};
#[cfg(not(target_arch = "wasm32"))]
use winit::event::{
    AxisId, DeviceEvent, DeviceId, ElementState, Ime, ModifiersState, StartCause, Touch,
};
#[cfg(not(target_arch = "wasm32"))]
use winit::window::{Theme, WindowId};

#[cfg(not(target_arch = "wasm32"))]
// TODO: can remove all these types when we upgrade to winit 0.29
#[derive(Debug, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Event<T> {
    NewEvents(StartCause),
    WindowEvent {
        window_id: WindowId,
        event: WindowEvent,
    },
    DeviceEvent {
        device_id: DeviceId,
        event: DeviceEvent,
    },
    UserEvent(T),
    Suspended,
    Resumed,
    MainEventsCleared,
    RedrawRequested(WindowId),
    RedrawEventsCleared,
    LoopDestroyed,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq)]
pub(crate) enum WindowEvent {
    Resized(PhysicalSize<u32>),
    Moved(PhysicalPosition<i32>),
    CloseRequested,
    Destroyed,
    DroppedFile(PathBuf),
    HoveredFile(PathBuf),
    HoveredFileCancelled,
    ReceivedCharacter(char),
    Focused(bool),
    KeyboardInput {
        device_id: DeviceId,
        input: winit::event::KeyboardInput,
        is_synthetic: bool,
    },
    ModifiersChanged(ModifiersState),
    Ime(Ime),
    CursorMoved {
        device_id: DeviceId,
        position: PhysicalPosition<f64>,
    },

    CursorEntered {
        device_id: DeviceId,
    },
    CursorLeft {
        device_id: DeviceId,
    },
    MouseWheel {
        device_id: DeviceId,
        delta: winit::event::MouseScrollDelta,
        phase: winit::event::TouchPhase,
    },
    MouseInput {
        device_id: DeviceId,
        state: ElementState,
        button: winit::event::MouseButton,
    },
    TouchpadMagnify {
        device_id: DeviceId,
        delta: f64,
        phase: winit::event::TouchPhase,
    },
    SmartMagnify {
        device_id: DeviceId,
    },
    TouchpadRotate {
        device_id: DeviceId,
        delta: f32,
        phase: winit::event::TouchPhase,
    },
    TouchpadPressure {
        device_id: DeviceId,
        pressure: f32,
        stage: i64,
    },
    AxisMotion {
        device_id: DeviceId,
        axis: AxisId,
        value: f64,
    },
    Touch(Touch),
    ScaleFactorChanged {
        scale_factor: f64,
        new_inner_size: PhysicalSize<u32>,
    },
    ThemeChanged(Theme),
    Occluded(bool),
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn convert_event<T>(event: winit::event::Event<'_, T>) -> Event<T> {
    match event {
        winit::event::Event::NewEvents(start_cause) => Event::NewEvents(start_cause),
        winit::event::Event::WindowEvent { window_id, event } => Event::WindowEvent {
            window_id,
            event: convert_window_event(event),
        },
        winit::event::Event::DeviceEvent { device_id, event } => {
            Event::DeviceEvent { device_id, event }
        }
        winit::event::Event::UserEvent(value) => Event::UserEvent(value),
        winit::event::Event::Suspended => Event::Suspended,
        winit::event::Event::Resumed => Event::Resumed,
        winit::event::Event::MainEventsCleared => Event::MainEventsCleared,
        winit::event::Event::RedrawRequested(window_id) => Event::RedrawRequested(window_id),
        winit::event::Event::RedrawEventsCleared => Event::RedrawEventsCleared,
        winit::event::Event::LoopDestroyed => Event::LoopDestroyed,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn convert_window_event(event: winit::event::WindowEvent<'_>) -> WindowEvent {
    match event {
        winit::event::WindowEvent::AxisMotion {
            device_id,
            axis,
            value,
        } => WindowEvent::AxisMotion {
            device_id,
            axis,
            value,
        },
        winit::event::WindowEvent::CloseRequested => WindowEvent::CloseRequested,
        winit::event::WindowEvent::CursorEntered { device_id } => {
            WindowEvent::CursorEntered { device_id }
        }
        winit::event::WindowEvent::CursorLeft { device_id } => {
            WindowEvent::CursorLeft { device_id }
        }
        winit::event::WindowEvent::CursorMoved {
            device_id,
            position,
            ..
        } => WindowEvent::CursorMoved {
            device_id,
            position,
        },
        winit::event::WindowEvent::Destroyed => WindowEvent::Destroyed,
        winit::event::WindowEvent::DroppedFile(path_buf) => WindowEvent::DroppedFile(path_buf),
        winit::event::WindowEvent::Focused(b) => WindowEvent::Focused(b),
        winit::event::WindowEvent::HoveredFile(path_buf) => WindowEvent::HoveredFile(path_buf),
        winit::event::WindowEvent::HoveredFileCancelled => WindowEvent::HoveredFileCancelled,
        winit::event::WindowEvent::Ime(ime) => WindowEvent::Ime(ime),
        winit::event::WindowEvent::KeyboardInput {
            device_id,
            input,
            is_synthetic,
        } => WindowEvent::KeyboardInput {
            device_id,
            input,
            is_synthetic,
        },
        winit::event::WindowEvent::ModifiersChanged(modifiers_state) => {
            WindowEvent::ModifiersChanged(modifiers_state)
        }
        winit::event::WindowEvent::MouseInput {
            device_id,
            state,
            button,
            ..
        } => WindowEvent::MouseInput {
            device_id,
            state,
            button,
        },
        winit::event::WindowEvent::MouseWheel {
            device_id,
            delta,
            phase,
            ..
        } => WindowEvent::MouseWheel {
            device_id,
            delta,
            phase,
        },
        winit::event::WindowEvent::Moved(new_position) => WindowEvent::Moved(new_position),
        winit::event::WindowEvent::Occluded(b) => WindowEvent::Occluded(b),
        winit::event::WindowEvent::ReceivedCharacter(char) => WindowEvent::ReceivedCharacter(char),
        winit::event::WindowEvent::Resized(new_size) => WindowEvent::Resized(new_size),
        winit::event::WindowEvent::ScaleFactorChanged {
            scale_factor,
            new_inner_size,
        } => WindowEvent::ScaleFactorChanged {
            scale_factor,
            new_inner_size: *new_inner_size,
        },
        winit::event::WindowEvent::SmartMagnify { device_id } => {
            WindowEvent::SmartMagnify { device_id }
        }
        winit::event::WindowEvent::ThemeChanged(theme) => WindowEvent::ThemeChanged(theme),
        winit::event::WindowEvent::Touch(touch) => WindowEvent::Touch(touch),
        winit::event::WindowEvent::TouchpadMagnify {
            device_id,
            delta,
            phase,
        } => WindowEvent::TouchpadMagnify {
            device_id,
            delta,
            phase,
        },
        winit::event::WindowEvent::TouchpadPressure {
            device_id,
            pressure,
            stage,
        } => WindowEvent::TouchpadPressure {
            device_id,
            pressure,
            stage,
        },
        winit::event::WindowEvent::TouchpadRotate {
            device_id,
            delta,
            phase,
        } => WindowEvent::TouchpadRotate {
            device_id,
            delta,
            phase,
        },
    }
}
