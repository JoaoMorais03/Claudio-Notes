//! Menu bar extra and Control-Option-Space global hotkey (macOS).

use anyhow::{Context, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const HOTKEY_HELP: &str =
    "Enable Claudio Notes in System Settings → Privacy & Security → Accessibility, then relaunch. Default hotkey is Control-Option-Space.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeEvent {
    Toggle,
    NewNote,
    OpenVault,
    Quit,
}

pub struct MacChrome {
    _tray: TrayIcon,
    _hotkeys: Option<GlobalHotKeyManager>,
    pub hotkey_error: Option<String>,
}

pub fn install() -> Result<MacChrome> {
    let tray = build_tray().context("install menu bar extra")?;
    let (hotkeys, hotkey_error) = match register_hotkey() {
        Ok(manager) => (Some(manager), None),
        Err(err) => {
            eprintln!("global hotkey failed: {err:#}");
            (None, Some(err.to_string()))
        }
    };
    Ok(MacChrome {
        _tray: tray,
        _hotkeys: hotkeys,
        hotkey_error,
    })
}

pub fn drain_events() -> Vec<ChromeEvent> {
    let mut events = Vec::new();
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        match event.id.as_ref() {
            "toggle" => events.push(ChromeEvent::Toggle),
            "new" => events.push(ChromeEvent::NewNote),
            "vault" => events.push(ChromeEvent::OpenVault),
            "quit" => events.push(ChromeEvent::Quit),
            _ => {}
        }
    }
    // Drain tray clicks so the channel does not fill; toggle lives on the menu
    // and the global hotkey so left-click can open the status menu.
    while let Ok(_event) = TrayIconEvent::receiver().try_recv() {}
    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
        if event.state == HotKeyState::Pressed {
            events.push(ChromeEvent::Toggle);
        }
    }
    events
}

fn build_tray() -> Result<TrayIcon> {
    let toggle = MenuItem::with_id("toggle", "Show / Hide", true, None);
    let new_note = MenuItem::with_id("new", "New Note", true, None);
    let vault = MenuItem::with_id("vault", "Open Vault in Finder", true, None);
    let quit = MenuItem::with_id("quit", "Quit", true, None);
    let menu = Menu::with_items(&[
        &toggle,
        &new_note,
        &vault,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .context("build menu bar menu")?;

    let mut builder = TrayIconBuilder::new()
        .with_tooltip("Claudio Notes")
        .with_title("N")
        .with_menu(Box::new(menu));
    if let Ok(icon) = note_icon() {
        builder = builder.with_icon(icon).with_icon_as_template(true);
    }
    builder.build().context("create status item")
}

fn register_hotkey() -> Result<GlobalHotKeyManager> {
    let manager = GlobalHotKeyManager::new().context("create global hotkey manager")?;
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
    manager
        .register(hotkey)
        .context("register Control-Option-Space")?;
    Ok(manager)
}

fn note_icon() -> Result<tray_icon::Icon> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let on = x >= 8 && x < 24 && y >= 6 && y < 26;
            if !on {
                continue;
            }
            let i = ((y * SIZE + x) * 4) as usize;
            rgba[i] = 255;
            rgba[i + 1] = 255;
            rgba[i + 2] = 255;
            rgba[i + 3] = 255;
        }
    }
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).context("build tray icon")
}
