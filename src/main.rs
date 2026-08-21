mod store;

use std::path::PathBuf;
use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Editor, EditorState, InputEvent};
use gpui_component::{ActiveTheme, Root, Selectable, Sizable, StyledExt, TitleBar, h_flex, v_flex};
use gpui_component_assets::Assets;

use store::{NoteSummary, NotesStore};

actions!(claudio_notes, [NewNote, HideNotes]);

struct NotesApp {
    store: NotesStore,
    notes: Vec<NoteSummary>,
    active_id: Option<String>,
    editor: Entity<EditorState>,
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

        let _subscriptions = vec![cx.subscribe(&editor, |this, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                this.schedule_save(cx);
            }
        })];

        let focus = editor.focus_handle(cx);
        window.defer(cx, move |window, cx| {
            focus.focus(window, cx);
        });

        Self {
            store,
            notes,
            active_id: Some(active.id),
            editor,
            dirty: false,
            save_gen: 0,
            error: None,
            _subscriptions,
        }
    }

    fn failed(window: &mut Window, cx: &mut Context<Self>, error: String) -> Self {
        let editor = cx.new(|cx| EditorState::new(window, cx).default_value(""));
        Self {
            store: NotesStore::open(std::env::temp_dir().join("claudio-notes-fallback"))
                .expect("fallback notes dir"),
            notes: Vec::new(),
            active_id: None,
            editor,
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
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }

    fn hide(&mut self, _: &HideNotes, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_save(window, cx);
        cx.hide();
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
        v_flex()
            .id("notes-sidebar")
            .w(px(220.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .overflow_y_scroll()
            .p_2()
            .gap_1()
            .children(self.notes.iter().map(|note| {
                let id = note.id.clone();
                let selected = self.active_id.as_deref() == Some(note.id.as_str());
                let label = if selected && self.dirty {
                    format!("• {}", note.title)
                } else {
                    note.title.clone()
                };
                Button::new(SharedString::from(format!("note:{id}")))
                    .ghost()
                    .small()
                    .w_full()
                    .label(label)
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_note(&id, window, cx);
                    }))
            }))
    }
}

fn main() {
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
            opts.kind = WindowKind::Normal;
            opts
        };
        cx.activate(true);
        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| NotesApp::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("open Claudio Notes window");
        })
        .detach();
    });
}
