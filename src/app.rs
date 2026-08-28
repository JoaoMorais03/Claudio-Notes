use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use claudio_notes::{filename_stem, folder_of, NoteSummary, NotesStore};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Editor, EditorState, Input, InputEvent, InputState};
use gpui_component::menu::PopupMenuItem;
use gpui_component::notification::Notification;
use gpui_component::sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem};
use gpui_component::{
    ActiveTheme, Icon, IconName, Root, Sizable, StyledExt, TitleBar, WindowExt, h_flex, v_flex,
};
use gpui_component_assets::Assets;

use crate::chrome::{self, ChromeEvent, MacChrome};

actions!(claudio_notes, [NewNote, HideNotes]);

const ROOT_GROUP: &str = "Notes";
const SIDEBAR_WIDTH: f32 = 248.;

#[derive(Clone)]
struct AppHandle {
    notes: WeakEntity<NotesApp>,
    window: WindowHandle<Root>,
}

impl Global for AppHandle {}

struct ChromeKeepAlive {
    _chrome: MacChrome,
}

impl Global for ChromeKeepAlive {}

pub fn run() {
    let app = gpui_platform::application().with_assets(Assets);
    app.run(move |cx| {
        gpui_component::init(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-n", NewNote, Some("claudio_notes")),
            KeyBinding::new("cmd-w", HideNotes, Some("claudio_notes")),
            KeyBinding::new("escape", HideNotes, Some("claudio_notes")),
        ]);

        let window_options = {
            let mut opts = TitleBar::window_options();
            opts.window_bounds = Some(WindowBounds::centered(size(px(960.), px(640.)), cx));
            opts.window_min_size = Some(size(px(640.), px(420.)));
            opts.kind = WindowKind::Floating;
            opts
        };
        cx.activate(true);
        cx.spawn(async move |cx| {
            let mut notes_entity: Option<Entity<NotesApp>> = None;
            let window = cx
                .open_window(window_options, |window, cx| {
                    let view = cx.new(|cx| NotesApp::new(window, cx));
                    notes_entity = Some(view.clone());
                    let notes = view.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        notes
                            .update(cx, |this, cx| this.hide(&HideNotes, window, cx))
                            .ok();
                        false
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("open Claudio Notes window");

            let notes = notes_entity.expect("notes view");
            let _ = cx.update(|cx| {
                cx.set_global(AppHandle {
                    notes: notes.downgrade(),
                    window,
                });
                match chrome::install() {
                    Ok(installed) => {
                        if installed.hotkey_error.is_some() {
                            let _ = window.update(cx, |_, window, cx| {
                                window.push_notification(
                                    Notification::warning(chrome::HOTKEY_HELP)
                                        .title("Global hotkey needs Accessibility"),
                                    cx,
                                );
                            });
                        }
                        cx.set_global(ChromeKeepAlive { _chrome: installed });
                    }
                    Err(err) => {
                        eprintln!("menu bar extra failed: {err:#}");
                    }
                }
            });

            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let events = chrome::drain_events();
                if events.is_empty() {
                    continue;
                }
                let _ = cx.update(|cx| dispatch_chrome(cx, &events));
            }
        })
        .detach();
    });
}

fn dispatch_chrome(cx: &mut App, events: &[ChromeEvent]) {
    let Some(runtime) = cx.try_global::<AppHandle>().cloned() else {
        return;
    };
    for event in events {
        match event {
            ChromeEvent::Toggle => {
                let _ = runtime.window.update(cx, |_, window, cx| {
                    let _ = runtime
                        .notes
                        .update(cx, |this, cx| this.toggle(window, cx));
                });
            }
            ChromeEvent::NewNote => {
                let _ = runtime.window.update(cx, |_, window, cx| {
                    let _ = runtime.notes.update(cx, |this, cx| {
                        this.show(window, cx);
                        this.new_note(&NewNote, window, cx);
                    });
                });
            }
            ChromeEvent::OpenVault => {
                let _ = runtime
                    .notes
                    .update(cx, |this, cx| this.open_vault(cx));
            }
            ChromeEvent::Quit => {
                let _ = runtime.window.update(cx, |_, window, cx| {
                    let _ = runtime.notes.update(cx, |this, cx| {
                        this.flush_save(window, cx);
                    });
                });
                cx.quit();
            }
        }
    }
}

struct NotesApp {
    store: NotesStore,
    notes: Vec<NoteSummary>,
    active_id: Option<String>,
    editor: Entity<EditorState>,
    search: Entity<InputState>,
    dirty: bool,
    save_gen: u64,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl NotesApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = match NotesStore::default_root().and_then(NotesStore::open) {
            Ok(store) => store,
            Err(err) => {
                return Self::failed(window, cx, err.to_string());
            }
        };

        let seed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("community-notes");
        if let Err(err) = store.seed_from(&seed) {
            eprintln!("seed failed: {err:#}");
        }

        let mut notes = store.list().unwrap_or_default();
        if notes.is_empty() {
            match store.create("Untitled") {
                Ok(note) => notes.push(note),
                Err(err) => return Self::failed(window, cx, err.to_string()),
            }
        }

        let active = notes[0].clone();
        let content = store.read(&active.id).unwrap_or_default();

        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(true)
                .soft_wrap(true)
                .default_value(content)
                .placeholder("Write a note. ⌘⇧↵ later for a code block.")
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes"));

        let mut subscriptions = vec![cx.subscribe(&editor, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                this.schedule_save(cx);
            }
        })];
        subscriptions.push(cx.subscribe(&search, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                this.error = None;
                cx.notify();
            }
        }));

        let focus = editor.focus_handle(cx);
        window.defer(cx, move |window, cx| {
            focus.focus(window, cx);
        });

        Self {
            store,
            notes,
            active_id: Some(active.id),
            editor,
            search,
            dirty: false,
            save_gen: 0,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    fn failed(window: &mut Window, cx: &mut Context<Self>, error: String) -> Self {
        let editor = cx.new(|cx| EditorState::new(window, cx).default_value(""));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes"));
        Self {
            store: NotesStore::open(std::env::temp_dir().join("claudio-notes-fallback"))
                .expect("fallback notes dir"),
            notes: Vec::new(),
            active_id: None,
            editor,
            search,
            dirty: false,
            save_gen: 0,
            error: Some(error),
            _subscriptions: Vec::new(),
        }
    }

    fn refresh_list(&mut self) {
        match self.store.list() {
            Ok(notes) => self.notes = notes,
            Err(err) => self.error = Some(err.to_string()),
        }
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    fn open_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_id.as_deref() == Some(id) {
            return;
        }
        self.flush_save(window, cx);
        match self.store.read(id) {
            Ok(content) => {
                self.editor.update(cx, |editor, cx| {
                    editor.set_highlighter("markdown", cx);
                    editor.set_value(content, window, cx);
                });
                self.active_id = Some(id.to_string());
                self.dirty = false;
                self.error = None;
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }

    fn new_note(&mut self, _: &NewNote, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_save(window, cx);
        match self.store.create("Untitled") {
            Ok(note) => {
                self.editor.update(cx, |editor, cx| {
                    editor.set_highlighter("markdown", cx);
                    editor.set_value(String::new(), window, cx);
                });
                self.active_id = Some(note.id);
                self.dirty = false;
                self.refresh_list();
                self.error = None;
                self.editor.focus_handle(cx).focus(window, cx);
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }

    fn hide(&mut self, _: &HideNotes, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_save(window, cx);
        cx.hide();
    }

    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.activate(true);
        window.activate_window();
        if self.query(cx).trim().is_empty() {
            self.search.focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() {
            self.hide(&HideNotes, window, cx);
        } else {
            self.show(window, cx);
        }
    }

    fn open_vault(&mut self, cx: &mut Context<Self>) {
        match open_path(self.store.root()) {
            Ok(()) => self.error = None,
            Err(err) => self.error = Some(err),
        }
        cx.notify();
    }

    fn copy_path(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.path_of(id) {
            Ok(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
                window.push_notification(Notification::success("Copied path"), cx);
                self.error = None;
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }

    fn reveal_in_finder(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.store.path_of(id).and_then(|path| {
            std::process::Command::new("open")
                .arg("-R")
                .arg(&path)
                .spawn()
                .map(|_| ())
                .map_err(|err| err.into())
        }) {
            Ok(()) => self.error = None,
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }

    fn delete_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let was_active = self.active_id.as_deref() == Some(id);
        if was_active {
            self.flush_save(window, cx);
        }
        match self.store.delete(id) {
            Ok(()) => {
                if was_active {
                    self.active_id = None;
                    self.dirty = false;
                }
                self.refresh_list();
                self.error = None;
                if was_active {
                    if let Some(next) = self.notes.first().cloned() {
                        self.open_note(&next.id, window, cx);
                    } else {
                        self.new_note(&NewNote, window, cx);
                    }
                }
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.save_gen += 1;
        let save_id = self.save_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            this.update(cx, |this, cx| {
                if this.save_gen == save_id {
                    this.flush_save_now(cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn flush_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.flush_save_now(cx);
    }

    fn flush_save_now(&mut self, cx: &mut Context<Self>) {
        if !self.dirty {
            return;
        }
        let Some(id) = self.active_id.clone() else {
            return;
        };
        let content = self.editor.read(cx).value().to_string();
        match self.store.write(&id, &content) {
            Ok(()) => {
                self.dirty = false;
                self.refresh_list();
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }

    fn grouped_notes(&self, cx: &App) -> Vec<(String, Vec<&NoteSummary>)> {
        let query = self.query(cx).trim().to_lowercase();
        let mut groups: BTreeMap<String, Vec<&NoteSummary>> = BTreeMap::new();
        for note in &self.notes {
            if !query.is_empty() {
                let id = note.id.to_lowercase();
                let title = note.title.to_lowercase();
                let stem = filename_stem(&note.id).to_lowercase();
                if !id.contains(&query) && !title.contains(&query) && !stem.contains(&query) {
                    continue;
                }
            }
            let folder = folder_of(&note.id);
            let group = if folder.is_empty() {
                ROOT_GROUP.to_string()
            } else {
                folder
            };
            groups.entry(group).or_default().push(note);
        }
        for notes in groups.values_mut() {
            notes.sort_by(|a, b| filename_stem(&a.id).cmp(&filename_stem(&b.id)));
        }
        let mut out = Vec::new();
        if let Some(root) = groups.remove(ROOT_GROUP) {
            out.push((ROOT_GROUP.to_string(), root));
        }
        out.extend(groups);
        out
    }
}

impl Render for NotesApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .key_context("claudio_notes")
            .on_action(cx.listener(Self::new_note))
            .on_action(cx.listener(Self::hide))
            .child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .pr_2()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().font_bold().child("Claudio Notes"))
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Button::new("new-note")
                                        .ghost()
                                        .small()
                                        .label("New")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.new_note(&NewNote, window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("hide-notes")
                                        .ghost()
                                        .small()
                                        .label("Hide")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.hide(&HideNotes, window, cx);
                                        })),
                                ),
                        ),
                ),
            )
            .when_some(self.error.clone(), |this, err| {
                this.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(err),
                )
            })
            .child(
                h_flex().flex_1().min_h_0().child(self.sidebar(cx)).child(
                    Editor::new(&self.editor)
                        .bordered(false)
                        .p_0()
                        .h_full()
                        .flex_1()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(cx.theme().mono_font_size),
                ),
            )
    }
}

impl NotesApp {
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let groups = self.grouped_notes(cx);
        let mut sidebar = Sidebar::<SidebarGroup<SidebarMenu>>::new("notes-sidebar")
            .collapsible(false)
            .w(px(SIDEBAR_WIDTH))
            .header(
                SidebarHeader::new().child(
                    Input::new(&self.search)
                        .cleanable(true)
                        .small()
                        .prefix(Icon::new(IconName::Search).small()),
                ),
            );

        for (group_name, notes) in groups {
            let items = notes.into_iter().map(|note| {
                let id = note.id.clone();
                let selected = self.active_id.as_deref() == Some(note.id.as_str());
                let stem = filename_stem(&note.id);
                let label = if selected && self.dirty {
                    format!("• {stem}")
                } else {
                    stem
                };
                let this = cx.entity().downgrade();
                SidebarMenuItem::new(label)
                    .active(selected)
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |this, _, window, cx| {
                            this.open_note(&id, window, cx);
                        }
                    }))
                    .context_menu(move |menu, _, _| {
                        let copy_id = id.clone();
                        let reveal_id = id.clone();
                        let delete_id = id.clone();
                        let copy_this = this.clone();
                        let reveal_this = this.clone();
                        let delete_this = this.clone();
                        menu.item(
                            PopupMenuItem::new("Copy Path").on_click(move |_, window, cx| {
                                let id = copy_id.clone();
                                copy_this
                                    .update(cx, |this, cx| this.copy_path(&id, window, cx))
                                    .ok();
                            }),
                        )
                        .item(
                            PopupMenuItem::new("Reveal in Finder").on_click(move |_, _, cx| {
                                let id = reveal_id.clone();
                                reveal_this
                                    .update(cx, |this, cx| this.reveal_in_finder(&id, cx))
                                    .ok();
                            }),
                        )
                        .item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                            let id = delete_id.clone();
                            delete_this
                                .update(cx, |this, cx| this.delete_note(&id, window, cx))
                                .ok();
                        }))
                    })
            });
            sidebar = sidebar.child(
                SidebarGroup::new(group_name).child(SidebarMenu::new().children(items)),
            );
        }

        sidebar
    }
}

fn open_path(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|err| err.to_string())
}
