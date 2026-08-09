use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::config;
use crate::hardware::ai_assistant::{self, AiError, AiReply, ModelInfo, ToolCall};
use crate::hardware::ai_actionlog;
use crate::hardware::ai_snapshot;
use crate::hardware::gpu;
use crate::i18n::{t, tf};
use crate::ui::{background, gpu_page};

/// Same idea as `background::run` but for a worker task that reports
/// incremental progress (model pull) before its final result - `on_update`
/// runs once per progress item, `on_done` once at the end.
fn spawn_streaming<T, F, U, D>(work: F, mut on_update: U, on_done: D)
where
    T: Send + 'static,
    F: FnOnce(mpsc::Sender<T>) -> Result<(), AiError> + Send + 'static,
    U: FnMut(T) + 'static,
    D: FnOnce(Result<(), AiError>) + 'static,
{
    let (tx, rx) = mpsc::channel::<T>();
    let (done_tx, done_rx) = mpsc::channel::<Result<(), AiError>>();
    std::thread::spawn(move || {
        let result = work(tx);
        let _ = done_tx.send(result);
    });
    let on_done = RefCell::new(Some(on_done));
    glib::timeout_add_local(Duration::from_millis(80), move || {
        while let Ok(item) = rx.try_recv() {
            on_update(item);
        }
        match done_rx.try_recv() {
            Ok(result) => {
                if let Some(f) = on_done.borrow_mut().take() {
                    f(result);
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

/// Who a chat line is attributed to - each gets its own color/weight (via
/// `gtk::TextTag`) so "what I asked" vs "what the machine answered" reads as
/// two distinct voices instead of one flat stream of text.
#[derive(Clone, Copy, PartialEq)]
enum ChatKind {
    You,
    Ai,
    System,
}

/// Creates the 3 chat tags on `buffer` the first time it's called for that
/// buffer (idempotent - `TextTagTable::add` would panic on a duplicate name,
/// so check `lookup` first).
fn ensure_chat_tags(buffer: &gtk::TextBuffer) {
    if buffer.tag_table().lookup("chat-you").is_some() {
        return;
    }
    buffer.create_tag(
        Some("chat-you"),
        &[("foreground", &"#ffffff"), ("weight", &700i32)],
    );
    buffer.create_tag(
        Some("chat-ai"),
        &[("foreground", &crate::ui::brand_theme::accent_hex()), ("weight", &700i32)],
    );
    buffer.create_tag(
        Some("chat-system"),
        &[("foreground", &"#777777"), ("style", &gtk::pango::Style::Italic)],
    );
}

fn chat_prefix(kind: ChatKind) -> String {
    match kind {
        ChatKind::You => format!("{}: ", t("ai_you_prefix")),
        ChatKind::Ai => format!("{}: ", t("ai_ai_prefix")),
        ChatKind::System => String::new(),
    }
}

fn tag_name(kind: ChatKind) -> &'static str {
    match kind {
        ChatKind::You => "chat-you",
        ChatKind::Ai => "chat-ai",
        ChatKind::System => "chat-system",
    }
}

/// Appends one line, color/weight-tagged by `kind` so the chat log visually
/// separates the user's own messages, the AI's replies, and system/status
/// text (thinking..., errors, applied/cancelled) instead of one flat stream.
fn append_chat(tv: &gtk::TextView, text: &str, kind: ChatKind) {
    if text.is_empty() {
        return;
    }
    let buffer = tv.buffer();
    ensure_chat_tags(&buffer);
    let mut end = buffer.end_iter();
    let line = format!("{}{}", chat_prefix(kind), text);
    buffer.insert_with_tags_by_name(&mut end, &line, &[tag_name(kind)]);
    buffer.insert(&mut end, if text.ends_with('\n') { "\n" } else { "\n\n" });
    let mark = buffer.create_mark(None, &buffer.end_iter(), false);
    tv.scroll_mark_onscreen(&mark);
}

fn ai_error_text(e: &AiError, base_url: &str) -> String {
    match e {
        AiError::Unreachable(_) => tf("ai_err_unreachable", &[base_url]),
        AiError::HttpStatus(_, msg) => tf("ai_err_http", &[msg]),
        AiError::NoToolCall => t("ai_err_no_tool_call").to_string(),
        AiError::UnknownTool(name) => tf("ai_err_unknown_tool", &[name]),
        AiError::InvalidArgs(_) => t("ai_err_invalid_args").to_string(),
    }
}

/// Writes to the chat log if one is visible (manual/chat triggers), or falls
/// back to the debug app log otherwise (periodic background trigger, which
/// has no chat view to write into - applog::info is a no-op when debug
/// logging is off, same as everywhere else in this codebase).
fn note(output: Option<&gtk::TextView>, text: &str, kind: ChatKind) {
    match output {
        Some(tv) => append_chat(tv, text, kind),
        None => crate::hardware::applog::info(text),
    }
}

/// Translates the internal trigger tag ("periodic"/"manual"/"chat") for the
/// action log - the log is user-facing (shown on the AI page), so it must
/// follow the app's language exactly like every other string, not leak the
/// English literal used internally to route logic.
fn source_label(source: &str) -> String {
    match source {
        "periodic" => t("ai_source_periodic").to_string(),
        "manual" => t("ai_source_manual").to_string(),
        "chat" => t("ai_source_chat").to_string(),
        other => other.to_string(),
    }
}

fn apply_tool(tool: ToolCall, desc: &str, output: Option<&gtk::TextView>, source: &str) {
    let src = source_label(source);
    // Validation pass #2, immediately before dispatch - never trust that
    // "it looked valid earlier" still holds.
    if let Err(e) = ai_assistant::validate(&tool) {
        note(output, &e, ChatKind::System);
        ai_actionlog::log(&tf("ai_log_rejected", &[&src, &e]));
        return;
    }
    match ai_assistant::execute(&tool) {
        Ok(()) => {
            note(output, desc, ChatKind::Ai);
            note(output, t("ai_success"), ChatKind::System);
            crate::hardware::applog::info("AI assistant: action applied");
            ai_actionlog::log(&tf("ai_log_applied", &[&src, desc]));
        }
        Err(e) => {
            note(output, &tf("ai_failed", &[&e]), ChatKind::System);
            crate::hardware::applog::error(&format!("AI assistant: execute failed: {}", e));
            ai_actionlog::log(&tf("ai_log_failed", &[&src, desc, &e]));
        }
    }
}

/// Runs an `AiReply` through the confirm-or-auto-apply gate shared by all 3
/// triggers (periodic timer, "Analisar agora", chat question). Must be
/// called on the GTK main thread (it touches widgets and may show a dialog).
/// `output` is `None` for the periodic background trigger (no chat view to
/// write into). `parent` is the app window - `choose()` only needs it to
/// find the toplevel to attach the dialog to. `source` labels the trigger
/// in the action log ("periodic" / "manual" / "chat").
fn handle_reply(
    reply: AiReply,
    output: Option<&gtk::TextView>,
    parent: &gtk::ApplicationWindow,
    model: &str,
    source: &'static str,
) {
    let src = source_label(source);
    if let Some(comment) = &reply.comment {
        note(output, &format!("({}) {}", model, comment), ChatKind::Ai);
        ai_actionlog::log(&tf("ai_log_replied", &[&src, model, comment]));
    }
    let Some(tool) = reply.action else { return };

    if let Err(e) = ai_assistant::validate(&tool) {
        note(output, &e, ChatKind::System);
        ai_actionlog::log(&tf("ai_log_invalid_action", &[&src, model, &e]));
        return;
    }
    let desc = ai_assistant::describe(&tool);
    ai_actionlog::log(&tf("ai_log_suggests", &[&src, model, &desc]));

    let cfg = config::load_app_config();
    if cfg.ai_auto_apply {
        apply_tool(tool, &desc, output, source);
        return;
    }

    let dialog = adw::AlertDialog::new(Some(t("ai_confirm_title")), Some(&desc));
    dialog.add_responses(&[("cancel", t("ai_cancel")), ("confirm", t("ai_confirm"))]);
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let output_c = output.cloned();
    dialog.choose(parent, gio::Cancellable::NONE, move |response| {
        if response != "confirm" {
            note(output_c.as_ref(), t("ai_cancelled"), ChatKind::System);
            ai_actionlog::log(&tf("ai_log_declined", &[&src, &desc]));
            return;
        }
        apply_tool(tool, &desc, output_c.as_ref(), source);
    });
}

fn trigger_verdict(
    question: Option<String>,
    output: Option<&gtk::TextView>,
    parent: &gtk::ApplicationWindow,
    source: &'static str,
) {
    let cfg = config::load_app_config();
    if !cfg.ai_assistant_enabled {
        note(output, t("ai_disabled_note"), ChatKind::System);
        return;
    }
    let base_url = cfg.ai_ollama_url.clone();
    let model = cfg.ai_model.clone();
    note(output, &tf("ai_thinking_model", &[&model]), ChatKind::System);
    let src = source_label(source);
    ai_actionlog::log(&match question.as_deref() {
        Some(q) => tf("ai_log_asking_question", &[&src, &model, q]),
        None => tf("ai_log_asking", &[&src, &model]),
    });

    let base_url_for_err = base_url.clone();
    let model_for_reply = model.clone();
    let output_c = output.cloned();
    let parent_c = parent.clone();

    background::run(
        move || ai_snapshot::ask_verdict(&base_url, &model, question.as_deref()),
        move |result| match result {
            Ok(reply) => handle_reply(reply, output_c.as_ref(), &parent_c, &model_for_reply, source),
            Err(e) => {
                crate::hardware::applog::error(&format!("AI assistant: {:?}", e));
                let msg = ai_error_text(&e, &base_url_for_err);
                note(output_c.as_ref(), &msg, ChatKind::System);
                ai_actionlog::log(&tf("ai_log_request_failed", &[&src, &msg]));
            }
        },
    );
}

/// Entry point for the periodic background monitor (registered in
/// window.rs's `build_main_ui`) - same gate/flow as the chat and manual
/// triggers, just with no chat view to log into (falls back to applog).
pub fn run_periodic_check(window: &gtk::ApplicationWindow) {
    trigger_verdict(None, None, window, "periodic");
}

fn build_chat_section(window: &gtk::ApplicationWindow, content: &gtk::Box) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(t("ai_page_chat_title")));
    title.add_css_class("settings-section-title");
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    title_row.append(&title);
    // Quick stop, right here on the page - no need to go into Settings to
    // immediately halt the background monitor and disable this chat when
    // it's not needed (Ollama is a real, non-trivial CPU/RAM cost while a
    // model is loaded and answering).
    let stop_btn = gtk::Button::with_label(t("ai_stop"));
    stop_btn.add_css_class("destructive-action");
    title_row.append(&stop_btn);
    section.append(&title_row);

    let output_scroll = gtk::ScrolledWindow::new();
    output_scroll.set_size_request(-1, 220);
    output_scroll.add_css_class("log-area");
    let output = gtk::TextView::new();
    output.set_editable(false);
    output.set_wrap_mode(gtk::WrapMode::WordChar);
    output.add_css_class("log-text");
    output_scroll.set_child(Some(&output));
    section.append(&output_scroll);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some(t("ai_command_placeholder")));
    entry.set_hexpand(true);
    let send_btn = gtk::Button::with_label(t("ai_send"));
    send_btn.add_css_class("accent-button");
    let analyze_btn = gtk::Button::with_label(t("ai_analyze_now"));
    row.append(&entry);
    row.append(&send_btn);
    row.append(&analyze_btn);
    section.append(&row);

    {
        let entry_c = entry.clone();
        let output_c = output.clone();
        let window_c = window.clone();
        send_btn.connect_clicked(move |_| {
            let question = entry_c.text().to_string();
            if question.trim().is_empty() {
                return;
            }
            append_chat(&output_c, &question, ChatKind::You);
            entry_c.set_text("");
            trigger_verdict(Some(question), Some(&output_c), &window_c, "chat");
        });
    }
    {
        // Enter in the entry acts like clicking Send.
        let send_btn_c = send_btn.clone();
        entry.connect_activate(move |_| {
            send_btn_c.emit_clicked();
        });
    }
    {
        let output_c = output.clone();
        let window_c = window.clone();
        analyze_btn.connect_clicked(move |_| {
            trigger_verdict(None, Some(&output_c), &window_c, "manual");
        });
    }
    {
        let content_c = content.clone();
        let window_c = window.clone();
        stop_btn.connect_clicked(move |_| {
            let mut cfg = config::load_app_config();
            cfg.ai_assistant_enabled = false;
            let _ = config::save_app_config(&cfg);
            crate::hardware::applog::info("AI assistant: stopped from the AI page");
            // Background monitor (window.rs's 60s timer) re-reads config on
            // every tick, so it stops within the next minute too. Swap this
            // page's own content to the disabled/"Ativar" view right away
            // rather than leaving dead controls behind.
            rebuild_content(&content_c, &window_c);
        });
    }

    section
}

/// Persistent audit trail of every AI trigger/reply/action (see
/// `hardware::ai_actionlog`) - unlike the chat view above, this survives
/// app restarts and captures the periodic background monitor too, so
/// nothing the AI does happens invisibly even if nobody was watching the
/// chat when it fired.
fn build_actionlog_section() -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    section.set_margin_top(20);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(t("ai_page_log_title")));
    title.add_css_class("settings-section-title");
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    title_row.append(&title);
    let refresh_btn = gtk::Button::with_label(t("ai_refresh_log"));
    title_row.append(&refresh_btn);
    section.append(&title_row);

    let log_scroll = gtk::ScrolledWindow::new();
    log_scroll.set_size_request(-1, 180);
    log_scroll.add_css_class("log-area");
    let log_view = gtk::TextView::new();
    log_view.set_editable(false);
    log_view.set_wrap_mode(gtk::WrapMode::WordChar);
    log_view.add_css_class("log-text");
    log_scroll.set_child(Some(&log_view));
    section.append(&log_scroll);

    let refresh_log = {
        let log_view = log_view.clone();
        move || {
            let content = ai_actionlog::read_all();
            let buffer = log_view.buffer();
            buffer.set_text(if content.is_empty() { t("ai_log_empty") } else { &content });
            let mark = buffer.create_mark(None, &buffer.end_iter(), false);
            log_view.scroll_mark_onscreen(&mark);
        }
    };
    refresh_log();
    {
        let refresh_log = refresh_log.clone();
        refresh_btn.connect_clicked(move |_| refresh_log());
    }
    // Auto-refresh while the page is open, so new entries from the
    // periodic background monitor (or from chat/manual triggers) show up
    // without needing a manual click. 10s is independent of whatever
    // ai_check_interval_min is set to - always at least that responsive,
    // never slower, and cheap (just a file read).
    //
    // This section gets torn down and rebuilt every time the "Parar"/
    // "Ativar" toggle fires `rebuild_content` - a plain glib timeout has no
    // idea its widgets were unparented, so without this check every toggle
    // would leak one more perpetually-firing timer. `log_view.root()` goes
    // None once it's no longer attached to the window, which is this
    // closure's cue to self-cancel instead of ticking forever.
    let log_view_alive_check = log_view.clone();
    glib::timeout_add_seconds_local(10, move || {
        if log_view_alive_check.root().is_none() {
            return glib::ControlFlow::Break;
        }
        if crate::app_state::is_window_visible() {
            refresh_log();
        }
        glib::ControlFlow::Continue
    });

    section
}

/// Live resource monitor for whatever Ollama is doing - same gauge widget
/// as the GPU page (`gpu_page::create_gpu_gauge`/`set_gauge_draw`), reusing
/// the exact same data source (`hardware::gpu::read_gpu_metrics`, already
/// cached 1.8s) since a GPU-accelerated Ollama model competes for the same
/// VRAM the GPU page already tracks. VRAM is the headline gauge (it's what
/// actually caps which models fit); GPU utilization is shown alongside for
/// context while a request is in flight.
fn build_resource_section() -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    section.set_margin_top(20);

    let title = gtk::Label::new(Some(t("ai_page_resources_title")));
    title.add_css_class("settings-section-title");
    title.set_halign(gtk::Align::Start);
    section.append(&title);

    if !crate::hardware::capabilities::get().nvidia_gpu {
        let note = gtk::Label::new(Some(t("ai_resources_no_gpu")));
        note.add_css_class("info-note");
        note.set_halign(gtk::Align::Start);
        note.set_wrap(true);
        section.append(&note);
        return section;
    }

    let gauges_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    gauges_row.set_halign(gtk::Align::Start);
    gauges_row.set_margin_top(4);

    let vram_gauge = gpu_page::create_gpu_gauge(t("gpu_vram"));
    let util_gauge = gpu_page::create_gpu_gauge(t("gpu_utilization"));
    gauges_row.append(&vram_gauge.0);
    gauges_row.append(&util_gauge.0);
    section.append(&gauges_row);

    let vram_detail = gtk::Label::new(None);
    vram_detail.add_css_class("info-note");
    vram_detail.set_halign(gtk::Align::Start);
    section.append(&vram_detail);

    let (vram_da, vram_label) = (vram_gauge.1, vram_gauge.2);
    let (util_da, util_label) = (util_gauge.1, util_gauge.2);
    let vram_da_alive_check = vram_da.clone();
    let update = move || {
        let m = gpu::read_gpu_metrics();
        let vram_pct = if m.vram_total_mb > 0 {
            (m.vram_used_mb as f64 / m.vram_total_mb as f64) * 100.0
        } else {
            0.0
        };
        vram_label.set_text(&format!("{}%", vram_pct as u32));
        gpu_page::set_gauge_draw(&vram_da, (vram_pct / 100.0).clamp(0.0, 1.0));
        vram_detail.set_text(&format!("{} / {} MB", m.vram_used_mb, m.vram_total_mb));

        util_label.set_text(&format!("{}%", m.util_gpu_pct));
        gpu_page::set_gauge_draw(&util_da, (m.util_gpu_pct as f64 / 100.0).clamp(0.0, 1.0));
    };
    update();
    // Same self-cancelling pattern as the action log's timer below - this
    // section is rebuilt on every "Parar"/"Ativar" toggle, so the timer
    // needs to notice its widgets are gone rather than ticking forever.
    glib::timeout_add_seconds_local(3, move || {
        if vram_da_alive_check.root().is_none() {
            return glib::ControlFlow::Break;
        }
        if !crate::app_state::is_window_visible() {
            return glib::ControlFlow::Continue;
        }
        update();
        glib::ControlFlow::Continue
    });

    section
}

/// One row of the installed-models list. `name_group`/`action_group` are
/// shared `gtk::SizeGroup`s across every row in the model manager (both the
/// installed list and the recommended-downloads list below it) so names and
/// action buttons line up in clean columns instead of each row sizing
/// itself independently - the "grid" look asked for, without an actual
/// `gtk::Grid` (a plain zebra-striped `Box` list is simpler to keep
/// consistent with the checkmark/no-button asymmetry between rows).
fn build_model_row(
    m: &ModelInfo,
    active_model: &str,
    index: usize,
    name_group: &gtk::SizeGroup,
    action_group: &gtk::SizeGroup,
    do_refresh: &Rc<dyn Fn()>,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("model-row");
    if index % 2 == 1 {
        row.add_css_class("model-row-alt");
    }

    let mb = m.size_bytes as f64 / (1024.0 * 1024.0);
    let is_active = m.name == active_model;

    let name_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let status_icon = gtk::Label::new(Some(if is_active { "\u{2713}" } else { " " }));
    if is_active {
        status_icon.add_css_class("status-success");
    }
    name_box.append(&status_icon);
    let name_label = gtk::Label::new(Some(&m.name));
    name_label.add_css_class("model-row-name");
    name_label.set_halign(gtk::Align::Start);
    if is_active {
        name_label.add_css_class("status-success");
    }
    name_box.append(&name_label);
    name_group.add_widget(&name_box);
    row.append(&name_box);

    let meta_text = if is_active {
        format!("{:.0} MB - {}", mb, t("ai_active_model"))
    } else {
        format!("{:.0} MB", mb)
    };
    let meta_label = gtk::Label::new(Some(&meta_text));
    meta_label.add_css_class("model-row-meta");
    meta_label.set_halign(gtk::Align::End);
    meta_label.set_hexpand(true);
    row.append(&meta_label);

    // Rows without a button (the active model) still get an empty
    // same-size placeholder in the action slot, so the button column
    // doesn't jump left on rows that don't have one.
    let action_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    action_group.add_widget(&action_slot);
    if !is_active {
        let select_btn = gtk::Button::with_label(t("ai_select_model"));
        let name = m.name.clone();
        let do_refresh = do_refresh.clone();
        select_btn.connect_clicked(move |btn| {
            // Immediate feedback - the actual row (with the checkmark moved
            // over) only appears once do_refresh()'s async re-fetch lands.
            btn.set_sensitive(false);
            btn.set_label(t("ai_selecting"));
            let mut cfg = config::load_app_config();
            cfg.ai_model = name.clone();
            let _ = config::save_app_config(&cfg);
            do_refresh();
        });
        action_slot.append(&select_btn);
    }
    row.append(&action_slot);
    row
}

fn start_pull(name: String, progress_label: gtk::Label, do_refresh: Rc<dyn Fn()>) {
    let cfg = config::load_app_config();
    let base_url = cfg.ai_ollama_url.clone();
    let base_url_for_err = base_url.clone();
    progress_label.set_text(t("ai_downloading"));

    let progress_label_update = progress_label.clone();
    spawn_streaming(
        move |tx| ai_assistant::pull_model(&base_url, &name, move |p| { let _ = tx.send(p); }),
        move |p| {
            let text = if p.total > 0 {
                format!("{} {}%", p.status, p.completed * 100 / p.total.max(1))
            } else {
                p.status.clone()
            };
            progress_label_update.set_text(&text);
        },
        move |result| match result {
            Ok(()) => {
                progress_label.set_text(t("ai_download_done"));
                do_refresh();
            }
            Err(e) => progress_label.set_text(&ai_error_text(&e, &base_url_for_err)),
        },
    );
}

fn build_download_row(
    name: &'static str,
    tools_supported: bool,
    index: usize,
    name_group: &gtk::SizeGroup,
    action_group: &gtk::SizeGroup,
    do_refresh: &Rc<dyn Fn()>,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("model-row");
    if index % 2 == 1 {
        row.add_css_class("model-row-alt");
    }

    let name_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let name_label = gtk::Label::new(Some(name));
    name_label.add_css_class("model-row-name");
    name_label.set_halign(gtk::Align::Start);
    name_box.append(&name_label);
    name_group.add_widget(&name_box);
    row.append(&name_box);

    let meta_label = gtk::Label::new(Some(if tools_supported {
        t("ai_model_works")
    } else {
        t("ai_model_no_tools")
    }));
    meta_label.add_css_class("model-row-meta");
    if tools_supported {
        meta_label.add_css_class("status-success");
    }
    meta_label.set_halign(gtk::Align::End);
    meta_label.set_hexpand(true);
    row.append(&meta_label);

    let progress_label = gtk::Label::new(None);
    progress_label.add_css_class("model-row-meta");
    row.append(&progress_label);

    let action_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    action_group.add_widget(&action_slot);
    let btn = gtk::Button::with_label(t("ai_download"));
    let do_refresh = do_refresh.clone();
    btn.connect_clicked(move |_| {
        start_pull(name.to_string(), progress_label.clone(), do_refresh.clone());
    });
    action_slot.append(&btn);
    row.append(&action_slot);
    row
}

fn build_model_manager_section() -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    section.set_margin_top(20);

    let title = gtk::Label::new(Some(t("ai_page_models_title")));
    title.add_css_class("settings-section-title");
    title.set_halign(gtk::Align::Start);
    section.append(&title);

    let min_note = gtk::Label::new(Some(&tf("ai_model_min_note", &[ai_assistant::DEFAULT_MODEL])));
    min_note.add_css_class("info-note");
    min_note.set_halign(gtk::Align::Start);
    section.append(&min_note);

    let active_model_label = gtk::Label::new(Some(&tf(
        "ai_active_model_now",
        &[&config::load_app_config().ai_model],
    )));
    active_model_label.add_css_class("settings-row-title");
    active_model_label.set_halign(gtk::Align::Start);
    section.append(&active_model_label);

    let status_label = gtk::Label::new(None);
    status_label.add_css_class("info-note");
    status_label.set_halign(gtk::Align::Start);
    section.append(&status_label);

    let installed_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    section.append(&installed_box);

    let refresh_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let refresh_btn = gtk::Button::with_label(t("ai_refresh_models"));
    let test_btn = gtk::Button::with_label(t("ai_test_connection"));
    refresh_row.append(&refresh_btn);
    refresh_row.append(&test_btn);
    section.append(&refresh_row);

    // `do_refresh` needs to be passed into each row so its own "Selecionar"
    // button can trigger a re-refresh - but that means the closure has to
    // reference itself, which can't be captured before the `Rc` exists yet.
    // Worked around with an indirection cell: filled in right after
    // `do_refresh` is constructed, read back (well after that point) from
    // inside the async callback.
    let do_refresh_cell: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    let do_refresh: Rc<dyn Fn()> = {
        let installed_box = installed_box.clone();
        let status_label = status_label.clone();
        let active_model_label = active_model_label.clone();
        let do_refresh_cell = do_refresh_cell.clone();
        Rc::new(move || {
            let cfg = config::load_app_config();
            let base_url = cfg.ai_ollama_url.clone();
            let active_model = cfg.ai_model.clone();
            status_label.set_text(t("ai_loading_models"));
            active_model_label.set_text(&tf("ai_active_model_now", &[&active_model]));

            let installed_box_c = installed_box.clone();
            let status_label_c = status_label.clone();
            let base_url_for_err = base_url.clone();
            let do_refresh_cell = do_refresh_cell.clone();
            background::run(
                move || ai_assistant::list_installed_models(&base_url),
                move |result| {
                    let do_refresh_for_rows = do_refresh_cell
                        .borrow()
                        .clone()
                        .expect("do_refresh_cell filled right after construction");
                    while let Some(child) = installed_box_c.first_child() {
                        installed_box_c.remove(&child);
                    }
                    match result {
                        Ok(models) => {
                            status_label_c
                                .set_text(&tf("ai_models_found", &[&models.len().to_string()]));
                            // Fresh SizeGroups every rebuild (not shared
                            // across refreshes): a SizeGroup holds a
                            // reference to every widget added to it, so
                            // reusing one across rebuilds would keep every
                            // discarded old row alive forever. New ones
                            // here means the previous batch's rows (and
                            // their SizeGroup) are dropped together once
                            // nothing else references them.
                            let name_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
                            let action_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
                            for (i, m) in models.iter().enumerate() {
                                installed_box_c.append(&build_model_row(
                                    m,
                                    &active_model,
                                    i,
                                    &name_group,
                                    &action_group,
                                    &do_refresh_for_rows,
                                ));
                            }
                        }
                        Err(e) => status_label_c.set_text(&ai_error_text(&e, &base_url_for_err)),
                    }
                },
            );
        })
    };
    *do_refresh_cell.borrow_mut() = Some(do_refresh.clone());
    do_refresh();
    {
        let do_refresh = do_refresh.clone();
        refresh_btn.connect_clicked(move |_| do_refresh());
    }
    {
        let status_label = status_label.clone();
        test_btn.connect_clicked(move |_| {
            let cfg = config::load_app_config();
            let base_url = cfg.ai_ollama_url.clone();
            let base_url_for_err = base_url.clone();
            status_label.set_text(t("ai_testing_connection"));
            let status_label_c = status_label.clone();
            background::run(
                move || ai_assistant::list_installed_models(&base_url),
                move |result| {
                    let text = match result {
                        Ok(_) => t("ai_connection_ok").to_string(),
                        Err(e) => ai_error_text(&e, &base_url_for_err),
                    };
                    status_label_c.set_text(&text);
                },
            );
        });
    }

    // Curated download shortlist, focused on low-footprint SmolLM2 variants.
    let recommended_title = gtk::Label::new(Some(t("ai_recommended_models")));
    recommended_title.set_halign(gtk::Align::Start);
    recommended_title.set_margin_top(12);
    section.append(&recommended_title);

    let recommended_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    section.append(&recommended_box);
    // This list is static (built once, never rebuilt), so - unlike the
    // installed-models list above - one long-lived pair of SizeGroups here
    // is fine, no accumulation risk.
    let rec_name_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
    let rec_action_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
    for (i, (name, tools_supported)) in ai_assistant::RECOMMENDED_MODELS.iter().enumerate() {
        recommended_box.append(&build_download_row(
            name,
            *tools_supported,
            i,
            &rec_name_group,
            &rec_action_group,
            &do_refresh,
        ));
    }

    // Free-text field for any other model name.
    let custom_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    custom_row.set_margin_top(8);
    let custom_entry = gtk::Entry::new();
    custom_entry.set_placeholder_text(Some(t("ai_custom_model_placeholder")));
    custom_entry.set_hexpand(true);
    let custom_download_btn = gtk::Button::with_label(t("ai_download"));
    custom_row.append(&custom_entry);
    custom_row.append(&custom_download_btn);
    section.append(&custom_row);

    let custom_progress_label = gtk::Label::new(None);
    custom_progress_label.set_halign(gtk::Align::Start);
    section.append(&custom_progress_label);

    {
        let custom_entry_c = custom_entry.clone();
        let custom_progress_label = custom_progress_label.clone();
        let do_refresh = do_refresh.clone();
        custom_download_btn.connect_clicked(move |_| {
            let name = custom_entry_c.text().to_string();
            if name.trim().is_empty() {
                return;
            }
            start_pull(name, custom_progress_label.clone(), do_refresh.clone());
        });
    }

    section
}

/// Clears `content` and rebuilds it from the CURRENT config - either the
/// disabled note + an "Ativar" button, or the chat + model manager. Called
/// once at page build time and again every time enabled state flips from
/// either the "Parar" button here or the "Ativar" button here, so the page
/// reflects the change immediately with no app restart needed (unlike the
/// Settings page's own enable switch, which this page doesn't hear about -
/// a real gap, but a separate one from what this fixes).
fn rebuild_content(content: &gtk::Box, window: &gtk::ApplicationWindow) {
    while let Some(child) = content.first_child() {
        content.remove(&child);
    }

    // Shown regardless of enabled state - it's a record of what already
    // happened (including from before the feature was turned off), not
    // part of the live chat/control surface above.
    content.append(&build_actionlog_section());

    if !config::load_app_config().ai_assistant_enabled {
        let note = gtk::Label::new(Some(t("ai_disabled_note")));
        note.add_css_class("info-note");
        note.set_halign(gtk::Align::Start);
        note.set_margin_top(12);
        content.append(&note);

        let activate_btn = gtk::Button::with_label(t("ai_activate"));
        activate_btn.add_css_class("accent-button");
        activate_btn.set_halign(gtk::Align::Start);
        activate_btn.set_margin_top(8);
        let content_c = content.clone();
        let window_c = window.clone();
        activate_btn.connect_clicked(move |_| {
            let mut cfg = config::load_app_config();
            cfg.ai_assistant_enabled = true;
            let _ = config::save_app_config(&cfg);
            rebuild_content(&content_c, &window_c);
        });
        content.append(&activate_btn);
        return;
    }

    content.append(&build_chat_section(window, content));
    content.append(&build_resource_section());
    content.append(&build_model_manager_section());
}

pub fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let title = gtk::Label::new(Some(t("ai_page_title")));
    title.add_css_class("page-title");
    title.set_halign(gtk::Align::Start);
    page.append(&title);

    let desc = gtk::Label::new(Some(t("ai_assistant_desc")));
    desc.add_css_class("info-note");
    desc.set_halign(gtk::Align::Start);
    desc.set_wrap(true);
    page.append(&desc);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.append(&content);
    rebuild_content(&content, window);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&page));
    scroll
}
