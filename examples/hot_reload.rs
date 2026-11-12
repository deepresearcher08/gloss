//! M9: live-reload the styles crate without losing signal state.
//!
//! Run a rebuild watcher in one terminal and the app in another:
//!
//! ```sh
//! cargo watch -w crates/gloss-styles -x "build -p gloss-styles"
//! cargo run -p gloss --example hot_reload
//! ```
//!
//! Edit `crates/gloss-styles/src/lib.rs` (for example the card radius, tint or
//! accent) and the running window restyles in place. The counter keeps its
//! value because the retained tree and its signals are never rebuilt.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gloss::{
    Align, App, ButtonVariant, Color, Edges, FlexDirection, HotReload, Node, NodeId, TextStyle,
    button, signal,
};
use gloss_styles::{StyleSet, styles};

struct Handles {
    root: NodeId,
    card: NodeId,
    title: NodeId,
    body: NodeId,
    pill: NodeId,
    counter: NodeId,
}

fn lib_dir() -> PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(profile)
}

fn main() {
    let handles = Rc::new(RefCell::new(None::<Handles>));
    let view_handles = Rc::clone(&handles);
    let reload_handles = Rc::clone(&handles);

    let mut app = App::new()
        .title("Gloss - Hot Reload")
        .size(760.0, 560.0)
        .debug(true);

    match HotReload::<StyleSet>::new(lib_dir(), "gloss_styles", "gloss_styles") {
        Ok(reloader) => {
            app = app.hot_reload(reloader, move |doc, set| {
                let handles = reload_handles.borrow();
                let Some(handles) = handles.as_ref() else {
                    return;
                };
                doc.set_style(handles.root, set.background);
                doc.set_style(handles.card, set.card);
                doc.set_style(handles.pill, set.pill);
                if let Some(node) = doc.get_mut(handles.title) {
                    node.text_style = Some(set.title);
                }
                if let Some(node) = doc.get_mut(handles.body) {
                    node.text_style = Some(set.body);
                }
                if let Some(node) = doc.get_mut(handles.counter) {
                    node.text_style = Some(TextStyle::new(24.0).weight(700).color(set.accent));
                }
            });
        }
        Err(error) => {
            eprintln!("hot reload disabled (build gloss-styles first): {error}");
        }
    }

    app.run(move |doc| {
        let set = styles();
        let root = doc.root();
        doc.set_style(
            root,
            set.background
                .clone()
                .align_items(Align::Center)
                .justify_content(Align::Center),
        );

        let card = doc
            .add(
                root,
                Node::panel(set.card.clone().width(480.0).align_items(Align::Center)),
            )
            .expect("add card");

        let title = doc
            .add(card, Node::label("Gloss hot reload", set.title))
            .expect("add title");
        let body = doc
            .add(
                card,
                Node::label(
                    "Edit crates/gloss-styles/src/lib.rs. The card, pill and text restyle \
                     in place while the counter keeps its value.",
                    set.body,
                ),
            )
            .expect("add body");

        let pill = doc
            .add(
                card,
                Node::panel(
                    set.pill
                        .clone()
                        .align_items(Align::Center)
                        .justify_content(Align::Center),
                ),
            )
            .expect("add pill");

        let count = signal(0_i32);
        let counter = doc
            .add(
                pill,
                Node::label(
                    "Count 0",
                    TextStyle::new(24.0).weight(700).color(set.accent),
                ),
            )
            .expect("add counter");
        doc.bind_text(counter, move || format!("Count {}", count.get()));

        let increment = Node::button(
            "Increment",
            button(&set.theme, ButtonVariant::Primary)
                .padding(Edges::symmetric(26.0, 12.0))
                .flex_direction(FlexDirection::Row),
        )
        .on_click(move || count.update(|value| *value += 1));
        doc.add(card, increment);

        doc.add(
            card,
            Node::label(
                "F1 toggles the debug overlay.",
                TextStyle::new(12.0).color(Color::rgba(0xFFFFFF, 0.35)),
            ),
        );

        *view_handles.borrow_mut() = Some(Handles {
            root,
            card,
            title,
            body,
            pill,
            counter,
        });
    });
}
